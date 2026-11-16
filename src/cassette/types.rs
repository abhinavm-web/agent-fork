use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents all recorded LLM/MCP turns for a specific step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepCassette {
    pub version: u32,
    pub session_id: String,
    pub step_id: u64,
    pub branch: String,
    pub recorded_at: DateTime<Utc>,
    pub entries: Vec<CassetteEntry>,
}

impl StepCassette {
    pub fn new(session_id: &str, step_id: u64, branch: &str) -> Self {
        Self {
            version: 1,
            session_id: session_id.to_string(),
            step_id,
            branch: branch.to_string(),
            recorded_at: Utc::now(),
            entries: Vec::new(),
        }
    }
}

/// A single HTTP/MCP request-response exchange within a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassetteEntry {
    pub id: String,
    pub index: usize,
    pub timestamp: DateTime<Utc>,
    pub provider: LlmProvider,
    pub endpoint: String,
    pub signature: String,
    pub request: RecordedRequest,
    pub response: RecordedResponse,
    pub duration_ms: u64,
    pub is_edited: bool,
    pub edit_history: Option<Vec<EditRecord>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Anthropic,
    OpenAi,
    Mcp,
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedRequest {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body_json: Option<serde_json::Value>,
    pub body_raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordedResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub is_streaming: bool,
    pub stream_events: Option<Vec<SseEvent>>,
    pub body_raw: String,
    pub parsed_message: Option<NormalizedMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SseEvent {
    pub event: Option<String>,
    pub data: String,
    pub id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct NormalizedMessage {
    pub role: String,
    pub content_text: String,
    pub tool_calls: Vec<NormalizedToolCall>,
    pub stop_reason: Option<String>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditRecord {
    pub edited_at: DateTime<Utc>,
    pub edit_type: EditType,
    pub description: String,
    pub original_signature: String,
    pub new_signature: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EditType {
    PromptOverride,
    ToolOutputOverride,
    AssistantMessageOverride,
    FullTurnEdit,
}
