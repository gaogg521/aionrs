//! End-to-end reproduction of the output-truncation bug against a stubbed
//! OpenAI-compatible endpoint.
//!
//! Unlike the unit tests, this drives the *real* provider stack — HTTP
//! transport, SSE frame parsing, `finish_reason` mapping and the projector —
//! so it covers the layers a mocked `LlmProvider` skips.
//!
//! The stub emulates a gateway with a small output cap (DeepSeek defaults to
//! 4096 output tokens): every response is cut short with
//! `finish_reason: "length"` until the content is exhausted.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use aion_agent::engine::AgentEngine;
use aion_agent::output::OutputSink;
use aion_agent::output::terminal::TerminalSink;
use aion_config::compat::ProviderCompat;
use aion_config::config::{Config, ProviderType, SessionConfig, SkillsPermissionConfig, ToolsConfig};
use aion_config::hooks::HooksConfig;
use aion_mcp::config::McpConfig;
use aion_providers::create_provider;
use aion_tools::registry::ToolRegistry;
use aion_types::message::StopReason;
use serde_json::json;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, Respond, ResponseTemplate};

/// Lines of Python emitted per model turn before the stub hits its cap.
const LINES_PER_CHUNK: usize = 250;
/// Total lines the "model" wants to produce — the user's 3000-line request.
const TOTAL_LINES: usize = 3000;

fn sse_frame(value: serde_json::Value) -> String {
    format!("data: {value}\n\n")
}

/// Serves one chunk of the answer per request, cutting every chunk short with
/// `finish_reason: "length"` except the final one.
struct TruncatingEndpoint {
    calls: AtomicUsize,
}

impl Respond for TruncatingEndpoint {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        let start = call * LINES_PER_CHUNK;
        let end = ((call + 1) * LINES_PER_CHUNK).min(TOTAL_LINES);
        let is_last = end >= TOTAL_LINES;

        let mut body = String::new();
        for line in start..end {
            // Deliberately includes braces, quotes and backslashes: if SSE
            // framing were sensitive to payload characters, this would break.
            let content = format!("def f_{line}():\n    return {{\"k\": \"v\\\\{line}\"}}\n");
            body.push_str(&sse_frame(json!({
                "choices": [{ "index": 0, "delta": { "content": content } }]
            })));
        }

        let finish_reason = if is_last { "stop" } else { "length" };
        body.push_str(&sse_frame(json!({
            "choices": [{ "index": 0, "delta": {}, "finish_reason": finish_reason }]
        })));
        body.push_str(&sse_frame(json!({
            "choices": [],
            "usage": { "prompt_tokens": 100, "completion_tokens": 4096, "total_tokens": 4196 }
        })));
        body.push_str("data: [DONE]\n\n");

        ResponseTemplate::new(200)
            .set_body_raw(body, "text/event-stream")
            .append_header("content-type", "text/event-stream")
    }
}

fn stub_config(base_url: String) -> Config {
    Config {
        provider: ProviderType::OpenAI,
        provider_label: "deepseek-stub".to_string(),
        api_key: "test-key".to_string(),
        base_url,
        model: "deepseek-v4-flash".to_string(),
        // Mirrors the real defect: no per-model cap configured, so aionrs omits
        // max_tokens entirely and the gateway applies its own small default.
        max_tokens: None,
        max_turns: Some(10),
        max_tool_call_malformed_turns: Some(3),
        max_tool_call_failure_turns: Some(3),
        system_prompt: Some("You are a helpful assistant.".to_string()),
        thinking: None,
        prompt_caching: false,
        compat: ProviderCompat::openai_defaults(),
        tools: ToolsConfig {
            auto_approve: true,
            allow_list: vec![],
            skills: SkillsPermissionConfig::default(),
        },
        session: SessionConfig {
            enabled: false,
            directory: std::env::temp_dir().to_string_lossy().into_owned(),
            max_sessions: 1,
        },
        compact: aion_config::compact::CompactConfig::default(),
        plan: aion_config::plan::PlanConfig::default(),
        shell: aion_config::shell::ShellConfig::default(),
        file_cache: aion_config::file_cache::FileCacheConfig::default(),
        hooks: HooksConfig::default(),
        bedrock: None,
        vertex: None,
        mcp: McpConfig::default(),
        logging: aion_config::logging::LoggingConfig::default(),
    }
}

/// The reported bug: asking for 3000 lines from a provider with a small output
/// cap used to abort after a single failed continuation. The full answer must
/// now come back stitched together.
#[tokio::test]
async fn long_code_request_survives_repeated_output_truncation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(TruncatingEndpoint {
            calls: AtomicUsize::new(0),
        })
        .mount(&server)
        .await;

    let config = stub_config(server.uri());
    let provider = create_provider(&config);
    let output: Arc<dyn OutputSink> = Arc::new(TerminalSink::new(true));

    let mut engine =
        AgentEngine::new_with_provider(provider, config, ToolRegistry::new(), output, std::env::temp_dir());
    let result = engine
        .run("写一段3000行的Python代码，内容随意。", "")
        .await
        .expect("engine should recover the full answer");

    assert_eq!(
        result.stop_reason,
        StopReason::EndTurn,
        "a completed answer must not report MaxTokens"
    );

    let produced = result.text.matches("def f_").count();
    assert_eq!(
        produced, TOTAL_LINES,
        "every line must survive the truncation boundaries"
    );
    assert!(
        result.text.contains("def f_0()") && result.text.contains(&format!("def f_{}()", TOTAL_LINES - 1)),
        "the answer must span the first through the last line"
    );

    // Chunk boundaries are where a naive implementation drops or duplicates
    // content; check the seam between the first and second model turn.
    assert!(
        result.text.contains(&format!("def f_{}()", LINES_PER_CHUNK - 1))
            && result.text.contains(&format!("def f_{LINES_PER_CHUNK}()")),
        "no content may be lost across a truncation seam"
    );
    assert_eq!(
        result.text.matches("def f_0()").count(),
        1,
        "the continuation must not replay content the model already produced"
    );

    assert_eq!(
        server.received_requests().await.expect("requests recorded").len(),
        TOTAL_LINES.div_ceil(LINES_PER_CHUNK),
        "one request per chunk, no wasted turns"
    );
}
