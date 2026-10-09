//! The shared HTTP client: timeouts, and retries with backoff before streaming.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::{RequestBuilder, Response, StatusCode, header::RETRY_AFTER};

use crate::LlmError;

/// Longest wait between attempts, whatever `Retry-After` says.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Characters of an error body kept in the error message.
const MAX_ERROR_CHARS: usize = 300;

#[derive(Debug, Clone)]
pub(crate) struct Http {
    client: reqwest::Client,
    max_retries: u32,
    retry_base: Duration,
}

impl Http {
    pub(crate) fn new(
        connect_timeout: Duration,
        read_timeout: Duration,
        max_retries: u32,
        retry_base: Duration,
    ) -> Result<Self, LlmError> {
        // No overall timeout: answers stream for as long as they take, but every
        // read must make progress within `read_timeout`.
        let client = reqwest::Client::builder()
            .connect_timeout(connect_timeout)
            .read_timeout(read_timeout)
            .user_agent(concat!("akasha/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| LlmError::Config(format!("building the HTTP client: {e}")))?;
        Ok(Self {
            client,
            max_retries,
            retry_base,
        })
    }

    /// Send the request built by `build`, retrying connection errors, 429 and
    /// 5xx. Returns the response once it has a success status.
    pub(crate) async fn send(
        &self,
        build: impl Fn(&reqwest::Client) -> RequestBuilder,
    ) -> Result<Response, LlmError> {
        let mut attempt = 0;
        loop {
            let (err, retry_after) = match build(&self.client).send().await {
                Ok(res) if res.status().is_success() => return Ok(res),
                Ok(res) => {
                    let retry_after = retry_after(&res);
                    (status_error(res).await, retry_after)
                }
                Err(e) => (LlmError::from(e), None),
            };
            if attempt >= self.max_retries || !err.is_retryable() {
                return Err(err);
            }
            let wait = retry_after.unwrap_or_else(|| backoff(self.retry_base, attempt));
            tracing::warn!(%err, attempt, wait_ms = wait.as_millis() as u64, "LLM request failed, retrying");
            tokio::time::sleep(wait.min(MAX_BACKOFF)).await;
            attempt += 1;
        }
    }
}

/// Exponential backoff with up to 25 % jitter.
fn backoff(base: Duration, attempt: u32) -> Duration {
    let exp = base.saturating_mul(2u32.saturating_pow(attempt.min(10)));
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let jitter = exp.mul_f64(f64::from(nanos % 1000) / 4000.0);
    exp + jitter
}

/// `Retry-After` in seconds (the HTTP-date form is ignored).
fn retry_after(res: &Response) -> Option<Duration> {
    let secs: u64 = res
        .headers()
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse()
        .ok()?;
    Some(Duration::from_secs(secs))
}

/// Turn an error response into [`LlmError::Status`], keeping the provider's
/// message (`error.message`, `error` or the raw body, shortened).
pub(crate) async fn status_error(res: Response) -> LlmError {
    let status = res.status();
    let body = res.text().await.unwrap_or_default();
    LlmError::Status {
        status: status.as_u16(),
        message: error_message(status, &body),
    }
}

fn error_message(status: StatusCode, body: &str) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let from_json = parsed.as_ref().and_then(|v| {
        let e = v.get("error").unwrap_or(v);
        e.get("message")
            .and_then(|m| m.as_str())
            .or_else(|| e.as_str())
            .map(str::to_owned)
    });
    let text = from_json.unwrap_or_else(|| body.trim().to_owned());
    if text.is_empty() {
        return status.canonical_reason().unwrap_or("error").to_owned();
    }
    shorten(&text)
}

pub(crate) fn shorten(text: &str) -> String {
    if text.chars().count() <= MAX_ERROR_CHARS {
        return text.to_owned();
    }
    let mut s: String = text.chars().take(MAX_ERROR_CHARS).collect();
    s.push('…');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_messages_come_from_common_json_shapes() {
        let s = StatusCode::BAD_REQUEST;
        assert_eq!(
            error_message(
                s,
                r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad model"}}"#
            ),
            "bad model"
        );
        assert_eq!(
            error_message(s, r#"{"error":"model not found"}"#),
            "model not found"
        );
        assert_eq!(error_message(s, "plain"), "plain");
        assert_eq!(error_message(s, ""), "Bad Request");
        assert!(error_message(s, &"x".repeat(1000)).chars().count() <= MAX_ERROR_CHARS + 1);
    }

    #[test]
    fn backoff_grows() {
        let base = Duration::from_millis(100);
        assert!(backoff(base, 0) >= base && backoff(base, 0) < base * 2);
        assert!(backoff(base, 3) >= base * 8);
    }
}
