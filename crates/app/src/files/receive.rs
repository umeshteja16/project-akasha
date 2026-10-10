//! Streaming a new file into staging while checking it: size limit, type sniffed
//! from the first bytes (must be on the allow-list and agree with the extension),
//! and UTF-8 for text types. Used by uploads and by watched folders.

use akasha_core::Error;
use akasha_storage::{FinishedBlob, StagedBlob, Storage};
use bytes::Bytes;
use futures_util::{Stream, StreamExt};

use super::sniff::{self, Detected, SNIFF_LEN, TextValidator};

/// Stream `body` into a staged blob. On any error the staged bytes are removed.
pub async fn receive<S>(
    storage: &Storage,
    name: &str,
    limit: u64,
    body: S,
) -> Result<(FinishedBlob, Detected), Error>
where
    S: Stream<Item = Result<Bytes, Error>>,
{
    let mut staged = storage.stage().await.map_err(Error::from)?;
    match Receiver::new(name, limit).run(body, &mut staged).await {
        Ok(detected) => {
            let blob = staged.finish().await.map_err(Error::from)?;
            Ok((blob, detected))
        }
        Err(err) => {
            if let Err(abort_err) = staged.abort().await {
                tracing::warn!(err = %abort_err, "failed to abort staged upload");
            }
            Err(err)
        }
    }
}

struct Receiver<'a> {
    name: &'a str,
    limit: u64,
    received: u64,
    /// Bytes held back until there are enough to sniff.
    head: Vec<u8>,
    detected: Option<Detected>,
    text: Option<TextValidator>,
}

impl<'a> Receiver<'a> {
    fn new(name: &'a str, limit: u64) -> Self {
        Self {
            name,
            limit,
            received: 0,
            head: Vec::with_capacity(SNIFF_LEN),
            detected: None,
            text: None,
        }
    }

    async fn run<S>(mut self, body: S, staged: &mut StagedBlob) -> Result<Detected, Error>
    where
        S: Stream<Item = Result<Bytes, Error>>,
    {
        let mut body = std::pin::pin!(body);
        while let Some(chunk) = body.next().await {
            let chunk = chunk?;
            self.received += chunk.len() as u64;
            if self.received > self.limit {
                return Err(Error::payload_too_large(format!(
                    "file exceeds the upload limit of {} MiB",
                    self.limit / (1024 * 1024)
                )));
            }
            if self.detected.is_some() {
                self.write(staged, chunk).await?;
            } else {
                self.head.extend_from_slice(&chunk);
                if self.head.len() >= SNIFF_LEN {
                    self.sniff(staged).await?;
                }
            }
        }
        if self.detected.is_none() {
            if self.head.is_empty() {
                return Err(Error::bad_request("file is empty"));
            }
            self.sniff(staged).await?;
        }
        if self.text.take().is_some_and(|t| !t.finish()) {
            return Err(not_text());
        }
        self.detected
            .ok_or_else(|| Error::internal("upload type was not detected"))
    }

    async fn sniff(&mut self, staged: &mut StagedBlob) -> Result<(), Error> {
        let detected = sniff::detect(&self.head, self.name)?;
        if detected.is_text {
            self.text = Some(TextValidator::default());
        }
        self.detected = Some(detected);
        let head = Bytes::from(std::mem::take(&mut self.head));
        self.write(staged, head).await
    }

    async fn write(&mut self, staged: &mut StagedBlob, chunk: Bytes) -> Result<(), Error> {
        if let Some(text) = &mut self.text
            && !text.feed(&chunk)
        {
            return Err(not_text());
        }
        staged.write(chunk).await.map_err(Error::from)
    }
}

fn not_text() -> Error {
    Error::unsupported_media_type("text files must be UTF-8 without NUL bytes")
}
