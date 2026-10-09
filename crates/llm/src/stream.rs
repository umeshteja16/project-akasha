//! Turning a response body into [`ChatEvent`]s: line splitting that survives
//! arbitrary chunk boundaries (including inside a UTF-8 character), a
//! Server-Sent Events parser, and the driver every provider shares.

use std::collections::VecDeque;

use bytes::Bytes;
use futures_util::{Stream, StreamExt, stream, stream::BoxStream};

use crate::{ChatEvent, ChatStream, LlmError};

/// Splits bytes into lines (`\n` or `\r\n`), holding back an incomplete last line.
#[derive(Debug, Default)]
pub struct LineBuf {
    pending: Vec<u8>,
}

impl LineBuf {
    /// Append `chunk`; returns the lines it completed.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, LlmError> {
        self.pending.extend_from_slice(chunk);
        let mut lines = Vec::new();
        while let Some(pos) = self.pending.iter().position(|&b| b == b'\n') {
            let rest = self.pending.split_off(pos + 1);
            let mut line = std::mem::replace(&mut self.pending, rest);
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            lines.push(utf8(line)?);
        }
        Ok(lines)
    }

    /// The unterminated last line, if any.
    pub fn finish(&mut self) -> Result<Option<String>, LlmError> {
        let line = std::mem::take(&mut self.pending);
        if line.is_empty() {
            Ok(None)
        } else {
            utf8(line).map(Some)
        }
    }
}

fn utf8(bytes: Vec<u8>) -> Result<String, LlmError> {
    String::from_utf8(bytes).map_err(|_| LlmError::Protocol("response is not UTF-8".into()))
}

/// One Server-Sent Event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    /// The `event:` field; `None` for the default `message` type.
    pub event: Option<String>,
    /// `data:` lines joined with `\n`.
    pub data: String,
}

/// Incremental SSE parser (the subset providers use: `event`, `data`, comments).
#[derive(Debug, Default)]
pub struct SseParser {
    event: Option<String>,
    data: Option<String>,
}

impl SseParser {
    /// Feed one line; returns an event when `line` (blank) ends one.
    pub fn line(&mut self, line: &str) -> Option<SseEvent> {
        if line.is_empty() {
            return self.dispatch();
        }
        if line.starts_with(':') {
            return None;
        }
        let (field, value) = match line.split_once(':') {
            Some((f, v)) => (f, v.strip_prefix(' ').unwrap_or(v)),
            None => (line, ""),
        };
        match field {
            "event" => self.event = Some(value.to_owned()),
            "data" => match &mut self.data {
                Some(d) => {
                    d.push('\n');
                    d.push_str(value);
                }
                None => self.data = Some(value.to_owned()),
            },
            _ => {}
        }
        None
    }

    /// The last event when the stream ends without a blank line.
    pub fn finish(&mut self) -> Option<SseEvent> {
        self.dispatch()
    }

    fn dispatch(&mut self) -> Option<SseEvent> {
        let event = self.event.take();
        let data = self.data.take()?;
        Some(SseEvent { event, data })
    }
}

/// A provider's wire format: lines in, events out.
pub(crate) trait Decoder: Send + 'static {
    /// Handle one complete line of the body.
    fn line(&mut self, line: &str, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError>;

    /// The body ended. Push the final `Done` if the format allows ending this
    /// way, or fail.
    fn finish(&mut self, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError>;
}

/// A decoder for SSE formats: one call per event.
pub(crate) trait SseDecoder: Send + 'static {
    fn event(&mut self, event: SseEvent, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError>;
    fn finish(&mut self, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError>;
}

/// Adapts an [`SseDecoder`] to [`Decoder`].
pub(crate) struct Sse<D> {
    parser: SseParser,
    inner: D,
}

impl<D: SseDecoder> Sse<D> {
    pub(crate) fn new(inner: D) -> Self {
        Self {
            parser: SseParser::default(),
            inner,
        }
    }
}

impl<D: SseDecoder> Decoder for Sse<D> {
    fn line(&mut self, line: &str, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        match self.parser.line(line) {
            Some(event) => self.inner.event(event, out),
            None => Ok(()),
        }
    }

    fn finish(&mut self, out: &mut VecDeque<ChatEvent>) -> Result<(), LlmError> {
        if let Some(event) = self.parser.finish() {
            self.inner.event(event, out)?;
        }
        if out.iter().any(|e| matches!(e, ChatEvent::Done { .. })) {
            return Ok(());
        }
        self.inner.finish(out)
    }
}

struct State<B, D> {
    body: B,
    lines: LineBuf,
    decoder: D,
    out: VecDeque<ChatEvent>,
    /// Nothing more will be read (body ended or failed).
    ended: bool,
    /// `Done` was emitted: the stream is over.
    done: bool,
}

/// Decode `body` with `decoder`. The stream stops after the first `Done` or error;
/// a body that ends without `Done` (and that the decoder cannot finish) is an error.
pub(crate) fn drive<B, D>(body: B, decoder: D) -> ChatStream
where
    B: Stream<Item = Result<Bytes, LlmError>> + Send + Unpin + 'static,
    D: Decoder,
{
    let state = State {
        body,
        lines: LineBuf::default(),
        decoder,
        out: VecDeque::new(),
        ended: false,
        done: false,
    };
    Box::pin(stream::unfold(state, |mut st| async move {
        loop {
            if st.done {
                return None;
            }
            if let Some(event) = st.out.pop_front() {
                st.done = matches!(event, ChatEvent::Done { .. });
                return Some((Ok(event), st));
            }
            if st.ended {
                return None;
            }
            if let Err(err) = step(&mut st).await {
                st.ended = true;
                st.out.clear();
                return Some((Err(err), st));
            }
        }
    }))
}

/// Read one chunk (or the end of the body) into `st.out`.
async fn step<B, D>(st: &mut State<B, D>) -> Result<(), LlmError>
where
    B: Stream<Item = Result<Bytes, LlmError>> + Send + Unpin + 'static,
    D: Decoder,
{
    match st.body.next().await {
        Some(chunk) => {
            for line in st.lines.push(&chunk?)? {
                st.decoder.line(&line, &mut st.out)?;
            }
        }
        None => {
            st.ended = true;
            if let Some(line) = st.lines.finish()? {
                st.decoder.line(&line, &mut st.out)?;
            }
            // A blank line flushes a pending SSE event before finishing.
            st.decoder.line("", &mut st.out)?;
            if !st.out.iter().any(|e| matches!(e, ChatEvent::Done { .. })) {
                st.decoder.finish(&mut st.out)?;
            }
        }
    }
    Ok(())
}

/// The body of `res` as a stream of chunks.
pub(crate) fn body(res: reqwest::Response) -> BoxStream<'static, Result<Bytes, LlmError>> {
    res.bytes_stream()
        .map(|chunk| chunk.map_err(|e| LlmError::Unreachable(e.without_url().to_string())))
        .boxed()
}

/// Parse a JSON line or event payload.
pub(crate) fn json(text: &str) -> Result<serde_json::Value, LlmError> {
    serde_json::from_str(text)
        .map_err(|e| LlmError::Protocol(format!("invalid JSON in the stream: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_survive_any_split_including_inside_utf8() {
        let text = "first ✓ line\r\nsecond\n\nthird";
        let bytes = text.as_bytes();
        for split in 0..bytes.len() {
            let mut buf = LineBuf::default();
            let mut lines = buf.push(&bytes[..split]).expect("a");
            lines.extend(buf.push(&bytes[split..]).expect("b"));
            lines.extend(buf.finish().expect("c"));
            assert_eq!(
                lines,
                ["first ✓ line", "second", "", "third"],
                "split at {split}"
            );
        }
    }

    #[test]
    fn sse_events_join_data_lines_and_skip_comments() {
        let mut p = SseParser::default();
        let mut events = Vec::new();
        for line in [
            ": ping",
            "event: delta",
            "data: {\"a\":",
            "data:1}",
            "",
            "data: x",
            "",
        ] {
            events.extend(p.line(line));
        }
        assert_eq!(
            events,
            [
                SseEvent {
                    event: Some("delta".into()),
                    data: "{\"a\":\n1}".into()
                },
                SseEvent {
                    event: None,
                    data: "x".into()
                },
            ]
        );
        assert_eq!(p.finish(), None);
        p.line("data: tail");
        assert_eq!(p.finish().map(|e| e.data), Some("tail".into()));
    }
}
