use crate::llm::providers::error::Result;
use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::pin::Pin;

/// Message role for chat interactions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// Minimal chat message representation for this module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Optional usage stats.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmUsage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// LLM response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: String,
    pub usage: Option<LlmUsage>,
    pub tool_calls: Vec<String>,
    pub model: String,
    pub finish_reason: Option<String>,
}

/// Configuration placeholder for completion calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LlmConfig {}

/// Tool definition placeholder.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: Option<String>,
}

/// Stream type for streaming responses.
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<LlmResponse>> + Send>>;

/// Minimal provider trait for this module.
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, messages: &[ChatMessage], config: &LlmConfig) -> Result<LlmResponse>;
    async fn complete_stream(
        &self,
        messages: &[ChatMessage],
        config: &LlmConfig,
    ) -> Result<ResponseStream>;
    async fn complete_with_tools(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &LlmConfig,
    ) -> Result<LlmResponse>;
    fn name(&self) -> &'static str;
    fn available_models(&self) -> Vec<&'static str>;
}
