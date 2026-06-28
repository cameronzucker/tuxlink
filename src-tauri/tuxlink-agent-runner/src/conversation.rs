//! The running transcript handed to the `Provider` each turn (T1).

use serde::{Deserialize, Serialize};

use crate::types::{ToolCall, ToolOutcome};

/// One entry in the conversation transcript. A `Provider` adapter renders these
/// into whatever wire format its model expects (chat messages, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Message {
    /// The operator's initiating prompt.
    User(String),
    /// A final or intermediate assistant text turn.
    Assistant(String),
    /// A tool call the assistant emitted (recorded for context).
    ToolCall(ToolCall),
    /// The result of a tool call, fed back to the model on the next turn.
    /// `ok` distinguishes a successful result from an error/validation message
    /// so the adapter can label it (e.g. an OpenAI `tool` role with an error
    /// marker) without re-parsing the content.
    ToolResult { name: String, ok: bool, content: String },
}

/// The accumulated transcript. Grows as the loop runs; passed by reference to
/// each `Provider::turn` so the model sees prior tool results (COR-3 feeds
/// validation errors back through here).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conversation {
    messages: Vec<Message>,
}

impl Conversation {
    /// A fresh transcript seeded with the operator's prompt.
    pub fn new(user_msg: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::User(user_msg.into())],
        }
    }

    /// Read-only view of the transcript.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Append an assistant text turn.
    pub fn push_assistant(&mut self, text: impl Into<String>) {
        self.messages.push(Message::Assistant(text.into()));
    }

    /// Record that the assistant emitted a tool call.
    pub fn push_tool_call(&mut self, call: ToolCall) {
        self.messages.push(Message::ToolCall(call));
    }

    /// Feed a successful tool result back to the model.
    pub fn push_tool_result(&mut self, name: impl Into<String>, content: impl Into<String>) {
        self.messages.push(Message::ToolResult {
            name: name.into(),
            ok: true,
            content: content.into(),
        });
    }

    /// Feed an error / validation message back to the model (COR-3 re-prompt).
    pub fn push_tool_error(&mut self, name: impl Into<String>, content: impl Into<String>) {
        self.messages.push(Message::ToolResult {
            name: name.into(),
            ok: false,
            content: content.into(),
        });
    }

    /// Convenience: append the outcome of an invoked tool, choosing the ok/error
    /// channel based on the variant.
    pub fn push_outcome(&mut self, name: &str, outcome: &ToolOutcome) {
        match outcome {
            ToolOutcome::Ok(value) => self.push_tool_result(name, value.to_string()),
            ToolOutcome::Denied(reason) => {
                self.push_tool_error(name, format!("tool denied: {reason}"))
            }
            ToolOutcome::InvalidArgs(detail) => {
                self.push_tool_error(name, format!("invalid arguments: {detail}"))
            }
        }
    }

    /// Number of transcript entries (useful in tests).
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Whether the transcript is empty.
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }
}
