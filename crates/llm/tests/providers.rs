//! Each provider against a local mock server: the request it sends (path,
//! headers, body) and how it parses streamed answers split at awkward places.

mod mock;

use akasha_llm::{
    ChatEvent, ChatModel, ChatRequest, LlmError, Message, Provider, StopReason, Usage, build,
};
use futures_util::StreamExt;
use mock::{Mock, Reply, split};

fn request() -> ChatRequest {
    ChatRequest {
        system: "Answer from the sources.".into(),
        messages: vec![
            Message::user("first question"),
            Message::assistant("first answer"),
            Message::user("Привет, what is ✓?"),
        ],
        max_tokens: 256,
        temperature: Some(0.1),
        json: false,
    }
}

/// Collected text and the final event (or the error that ended the stream).
async fn run(model: &dyn ChatModel) -> Result<(String, StopReason, Usage), LlmError> {
    let mut stream = model.stream(&request()).await?;
    let mut text = String::new();
    while let Some(event) = stream.next().await {
        match event? {
            ChatEvent::Delta(d) => text.push_str(&d),
            ChatEvent::Done { stop, usage } => return Ok((text, stop, usage)),
        }
    }
    panic!("stream ended without Done or an error");
}

fn usage(input: u32, output: u32) -> Usage {
    Usage {
        input_tokens: Some(input),
        output_tokens: Some(output),
    }
}

#[tokio::test]
async fn ollama_ndjson() {
    let body = concat!(
        r#"{"model":"m","message":{"role":"assistant","content":"Hé "},"done":false}"#,
        "\n",
        r#"{"model":"m","message":{"role":"assistant","content":"✓ done"},"done":false}"#,
        "\n",
        r#"{"model":"m","message":{"role":"assistant","content":""},"done":true,"done_reason":"stop","prompt_eval_count":42,"eval_count":7}"#,
        "\n",
    );
    for size in [1, 7, 64, body.len()] {
        let mock = Mock::start(vec![Reply::ok(split(body, size))]).await;
        let model = build(&mock.options(Provider::Ollama, "llama3.1:8b"))
            .expect("build")
            .expect("model");
        let (text, stop, u) = run(model.as_ref()).await.expect("answer");
        assert_eq!(text, "Hé ✓ done", "chunk size {size}");
        assert_eq!((stop, u), (StopReason::EndTurn, usage(42, 7)));

        let req = &mock.requests()[0];
        assert_eq!(req.uri.path(), "/api/chat");
        assert_eq!(req.body["model"], "llama3.1:8b");
        assert_eq!(req.body["stream"], true);
        assert_eq!(req.body["options"]["num_predict"], 256);
        assert_eq!(req.body["options"]["num_ctx"], 8192);
        let messages = req.body["messages"].as_array().expect("messages");
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[3]["content"], "Привет, what is ✓?");
    }
}

#[tokio::test]
async fn ollama_errors_and_truncation() {
    let mock = Mock::start(vec![
        Reply::status(
            404,
            r#"{"error":"model \"x\" not found, try pulling it first"}"#,
        ),
        Reply::ok(split(
            "{\"message\":{\"content\":\"a\"},\"done\":false}\n{\"error\":\"out of memory\"}\n",
            5,
        )),
        Reply::ok(split(
            "{\"message\":{\"content\":\"a\"},\"done\":false}\n",
            5,
        )),
    ])
    .await;
    let model = build(&mock.options(Provider::Ollama, "x"))
        .expect("build")
        .expect("model");
    let err = run(model.as_ref()).await.expect_err("404");
    assert!(
        matches!(&err, LlmError::Status { status: 404, message } if message.contains("not found")),
        "{err}"
    );
    assert_eq!(mock.requests().len(), 1, "4xx is not retried");
    let err = run(model.as_ref()).await.expect_err("error line");
    assert!(
        matches!(&err, LlmError::Upstream(m) if m == "out of memory"),
        "{err}"
    );
    let err = run(model.as_ref()).await.expect_err("cut off");
    assert!(matches!(err, LlmError::Protocol(_)), "{err}");
}

const ANTHROPIC_STREAM: &str = "event: message_start\n\
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"usage\":{\"input_tokens\":120,\"output_tokens\":1}}}\n\n\
event: ping\ndata: {\"type\":\"ping\"}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"thinking\",\"thinking\":\"\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"thinking_delta\",\"thinking\":\"hidden\"}}\n\n\
event: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"Grounded \"}}\n\n\
event: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"text_delta\",\"text\":\"answer [1].\"}}\n\n\
event: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\n\
event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":15}}\n\n\
event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";

#[tokio::test]
async fn anthropic_messages_sse() {
    for size in [3, 50, ANTHROPIC_STREAM.len()] {
        let mock = Mock::start(vec![Reply::ok(split(ANTHROPIC_STREAM, size))]).await;
        let model = build(&mock.options(Provider::Anthropic, ""))
            .expect("build")
            .expect("model");
        let (text, stop, u) = run(model.as_ref()).await.expect("answer");
        assert_eq!(text, "Grounded answer [1].", "chunk size {size}");
        assert_eq!((stop, u), (StopReason::EndTurn, usage(120, 15)));

        let req = &mock.requests()[0];
        assert_eq!(req.uri.path(), "/v1/messages");
        assert_eq!(req.headers["x-api-key"], "sk-ant-test-key");
        assert_eq!(req.headers["anthropic-version"], "2023-06-01");
        assert_eq!(
            req.headers["anthropic-beta"],
            akasha_llm::anthropic::FALLBACK_BETA
        );
        assert_eq!(req.body["model"], akasha_llm::anthropic::DEFAULT_MODEL);
        assert_eq!(req.body["fallbacks"], "default");
        assert_eq!(req.body["system"], "Answer from the sources.");
        assert_eq!(req.body["max_tokens"], 256);
        assert_eq!(req.body["stream"], true);
        assert_eq!(req.body["output_config"]["effort"], "low");
        assert!(
            req.body.get("temperature").is_none(),
            "current Claude models reject it"
        );
        assert_eq!(req.body["messages"].as_array().map(Vec::len), Some(3));
        assert_eq!(req.body["messages"][1]["role"], "assistant");
    }
}

#[tokio::test]
async fn anthropic_refusal_errors_and_older_models() {
    let refusal = "event: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"refusal\"},\"usage\":{\"output_tokens\":0}}\n\n\
                   event: message_stop\ndata: {\"type\":\"message_stop\"}\n\n";
    let overloaded = "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n";
    let mock = Mock::start(vec![
        Reply::ok(split(refusal, 9)),
        Reply::ok(split(overloaded, 9)),
    ])
    .await;
    let model = build(&mock.options(Provider::Anthropic, "claude-haiku-4-5"))
        .expect("build")
        .expect("model");
    let (text, stop, _) = run(model.as_ref()).await.expect("refusal is not an error");
    assert_eq!((text.as_str(), stop), ("", StopReason::Refusal));
    let req = &mock.requests()[0];
    assert!(
        req.headers.get("anthropic-beta").is_none(),
        "no fallbacks for older models"
    );
    assert!(req.body.get("fallbacks").is_none());
    let err = run(model.as_ref()).await.expect_err("error event");
    assert!(
        matches!(&err, LlmError::Upstream(m) if m == "Overloaded"),
        "{err}"
    );
}

const GEMINI_STREAM: &str = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"thinking\",\"thought\":true}],\"role\":\"model\"}}]}\r\n\r\n\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"Réponse \"}],\"role\":\"model\"}}],\"usageMetadata\":{\"promptTokenCount\":80}}\r\n\r\n\
data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"[2].\"}],\"role\":\"model\"},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":80,\"candidatesTokenCount\":6}}\r\n\r\n";

#[tokio::test]
async fn gemini_sse_with_key_in_header_only() {
    for size in [4, GEMINI_STREAM.len()] {
        let mock = Mock::start(vec![Reply::ok(split(GEMINI_STREAM, size))]).await;
        let model = build(&mock.options(Provider::Gemini, ""))
            .expect("build")
            .expect("model");
        let (text, stop, u) = run(model.as_ref()).await.expect("answer");
        assert_eq!(text, "Réponse [2].", "chunk size {size}");
        assert_eq!((stop, u), (StopReason::EndTurn, usage(80, 6)));

        let req = &mock.requests()[0];
        assert_eq!(
            req.uri.path(),
            "/v1beta/models/gemini-2.5-flash:streamGenerateContent"
        );
        assert_eq!(
            req.uri.query(),
            Some("alt=sse"),
            "the key never goes in the URL"
        );
        assert_eq!(req.headers["x-goog-api-key"], "AIza-test-key");
        assert_eq!(
            req.body["systemInstruction"]["parts"][0]["text"],
            "Answer from the sources."
        );
        assert_eq!(req.body["contents"][1]["role"], "model");
        assert_eq!(
            req.body["contents"][2]["parts"][0]["text"],
            "Привет, what is ✓?"
        );
        assert_eq!(req.body["generationConfig"]["maxOutputTokens"], 256);
    }
}

#[tokio::test]
async fn gemini_safety_stop_is_a_refusal() {
    let body = "data: {\"candidates\":[{\"finishReason\":\"SAFETY\"}]}\n\n";
    let mock = Mock::start(vec![
        Reply::ok(split(body, 10)),
        Reply::ok(split("data: {\"candidates\":[]}\n\n", 10)),
    ])
    .await;
    let model = build(&mock.options(Provider::Gemini, "gemini-2.5-pro"))
        .expect("build")
        .expect("model");
    let (_, stop, _) = run(model.as_ref()).await.expect("answer");
    assert_eq!(stop, StopReason::Refusal);
    assert!(
        matches!(run(model.as_ref()).await, Err(LlmError::Protocol(_))),
        "no finish reason"
    );
}

const OPENAI_STREAM: &str = "data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"}}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Local \"}}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"model ✓\"},\"finish_reason\":\"length\"}]}\n\n\
data: {\"id\":\"c1\",\"choices\":[],\"usage\":{\"prompt_tokens\":30,\"completion_tokens\":4,\"total_tokens\":34}}\n\n\
data: [DONE]\n\n";

#[tokio::test]
async fn openai_compatible_sse() {
    for size in [6, OPENAI_STREAM.len()] {
        let mock = Mock::start(vec![Reply::ok(split(OPENAI_STREAM, size))]).await;
        let model = build(&mock.options(Provider::OpenAi, "qwen2.5-7b-instruct"))
            .expect("build")
            .expect("model");
        let (text, stop, u) = run(model.as_ref()).await.expect("answer");
        assert_eq!(text, "Local model ✓", "chunk size {size}");
        assert_eq!((stop, u), (StopReason::MaxTokens, usage(30, 4)));

        let req = &mock.requests()[0];
        assert_eq!(req.uri.path(), "/v1/chat/completions");
        assert_eq!(req.headers["authorization"], "Bearer sk-openai-test");
        assert_eq!(req.body["model"], "qwen2.5-7b-instruct");
        assert_eq!(req.body["messages"][0]["role"], "system");
        assert_eq!(req.body["stream_options"]["include_usage"], true);
        assert_eq!(req.body["max_tokens"], 256);
        let t = req.body["temperature"].as_f64().expect("temperature");
        assert!((t - 0.1).abs() < 1e-6);
    }
}

#[tokio::test]
async fn retries_429_and_5xx_before_streaming() {
    let ok = "{\"message\":{\"content\":\"ok\"},\"done\":true}\n";
    let mut limited = Reply::status(429, r#"{"error":"slow down"}"#);
    limited.headers.push(("retry-after", "0"));
    let mock = Mock::start(vec![
        limited,
        Reply::status(503, "busy"),
        Reply::ok(split(ok, 4)),
    ])
    .await;
    let model = build(&mock.options(Provider::Ollama, "m"))
        .expect("build")
        .expect("model");
    let (text, _, _) = run(model.as_ref()).await.expect("third attempt succeeds");
    assert_eq!(text, "ok");
    assert_eq!(mock.requests().len(), 3);

    let mock = Mock::start((0..4).map(|_| Reply::status(500, "boom")).collect()).await;
    let model = build(&mock.options(Provider::Ollama, "m"))
        .expect("build")
        .expect("model");
    let err = run(model.as_ref()).await.expect_err("gives up");
    assert!(matches!(err, LlmError::Status { status: 500, .. }));
    assert_eq!(err.code(), "llm_unavailable");
    assert_eq!(mock.requests().len(), 3, "one try plus two retries");
}

#[tokio::test]
async fn unreachable_servers_are_reported_as_such() {
    let mut opts = Mock::start(Vec::new()).await.options(Provider::Ollama, "m");
    // A port nothing listens on.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    opts.ollama_url = format!("http://{}", listener.local_addr().expect("addr"));
    drop(listener);
    opts.max_retries = 1;
    let model = build(&opts).expect("build").expect("model");
    let err = run(model.as_ref()).await.expect_err("refused");
    assert!(matches!(err, LlmError::Unreachable(_)), "{err}");
    assert_eq!(err.code(), "llm_unavailable");
}

#[tokio::test]
async fn json_mode_is_requested_where_the_api_has_one() {
    let ollama_body = concat!(
        r#"{"message":{"content":"{}"},"done":false}"#,
        "\n",
        r#"{"message":{"content":""},"done":true,"done_reason":"stop"}"#,
        "\n",
    );
    let cases = [
        (Provider::Ollama, ollama_body, "/format", Some("json")),
        (
            Provider::Gemini,
            GEMINI_STREAM,
            "/generationConfig/responseMimeType",
            Some("application/json"),
        ),
        (
            Provider::Anthropic,
            concat!(
                "event: message_stop\n",
                "data: {\"type\":\"message_stop\"}\n\n"
            ),
            "/output_config/format",
            None,
        ),
    ];
    for (provider, reply, pointer, expected) in cases {
        let mock = Mock::start(vec![Reply::ok(split(reply, reply.len()))]).await;
        let model = build(&mock.options(provider, ""))
            .expect("build")
            .expect("model");
        let mut req = request();
        req.json = true;
        // Only the request matters here; the stream may end any way.
        if let Ok(mut stream) = model.stream(&req).await {
            while stream.next().await.is_some() {}
        }
        let body = &mock.requests()[0].body;
        assert_eq!(
            body.pointer(pointer).and_then(|v| v.as_str()),
            expected,
            "{provider:?}"
        );
    }
}
