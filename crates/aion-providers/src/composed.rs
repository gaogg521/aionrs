use async_trait::async_trait;
#[cfg(test)]
use serde_json::Value;
use tokio::sync::mpsc;

use aion_config::compat::ProviderCompat;
use aion_types::llm::{LlmEvent, LlmRequest};

use crate::error::ProviderError;
use crate::provider::LlmProvider;
use crate::stream_runner::run_stream;
use crate::transport::ProviderTransport;

#[derive(Clone)]
pub(crate) struct ComposedProvider {
    transport: ProviderTransport,
    compat: ProviderCompat,
}

impl ComposedProvider {
    pub(crate) fn new(transport: ProviderTransport, compat: ProviderCompat) -> Self {
        Self { transport, compat }
    }

    #[cfg(test)]
    pub(crate) fn build_request_body(&self, request: &LlmRequest) -> Result<Value, ProviderError> {
        let (body, _) = self.transport.project_body(request, &self.compat)?;
        Ok(body)
    }
}

/// Some OpenAI-protocol gateways front a thinking-capable model through an
/// Anthropic-shaped validation layer: they reject the flat `reasoning_content`
/// replay format as "thinking not passed back", even though it was sent.
/// Detect that specific rejection so `stream()` can retry once with the
/// `content[].thinking` array-block replay format instead.
fn is_thinking_replay_format_error(message: &str) -> bool {
    let lower = message.to_lowercase();
    lower.contains("content[].thinking") && lower.contains("passed back")
}

impl ComposedProvider {
    async fn stream_with_compat(
        &self,
        request: &LlmRequest,
        compat: &ProviderCompat,
    ) -> Result<mpsc::Receiver<LlmEvent>, ProviderError> {
        let (body, tool_wire_shape) = self.transport.project_body(request, compat)?;

        tracing::debug!(target: "aion_providers", body = %serde_json::to_string_pretty(&body).unwrap_or_default(), "outgoing request");

        let transport = self.transport.clone();
        let compat_owned = compat.clone();
        let model = request.model.clone();
        let send = move || {
            let transport = transport.clone();
            let compat = compat_owned.clone();
            let body = body.clone();
            let model = model.clone();
            async move {
                let projected_request = transport.build_projected_request(&model, body, &compat, tool_wire_shape)?;
                transport.send(projected_request).await
            }
        };

        let decoder = self.transport.decoder(compat);
        let process = move |response, tx| async move { decoder.process(response, &tx).await };
        let retry_policy = self.transport.retry_policy();

        run_stream(send, process, retry_policy).await
    }
}

#[async_trait]
impl LlmProvider for ComposedProvider {
    async fn stream(&self, request: &LlmRequest) -> Result<mpsc::Receiver<LlmEvent>, ProviderError> {
        match self.stream_with_compat(request, &self.compat).await {
            Err(ProviderError::Api { message, .. })
                if !self.compat.thinking_replay_as_content_block() && is_thinking_replay_format_error(&message) =>
            {
                tracing::warn!(
                    target: "aion_providers",
                    "gateway rejected reasoning_content thinking replay; retrying once with content[].thinking blocks"
                );
                let content_block_compat = self.compat.with_thinking_replay_as_content_block();
                match self.stream_with_compat(request, &content_block_compat).await {
                    Err(ProviderError::Api { message, .. }) if is_thinking_replay_format_error(&message) => {
                        tracing::warn!(
                            target: "aion_providers",
                            "gateway also rejected content[].thinking block replay; retrying once with thinking omitted entirely"
                        );
                        let omit_compat = self.compat.with_thinking_replay_omitted();
                        self.stream_with_compat(request, &omit_compat).await
                    }
                    other => other,
                }
            }
            other => other,
        }
    }
}

#[cfg(test)]
#[path = "composed_test.rs"]
mod composed_test;
