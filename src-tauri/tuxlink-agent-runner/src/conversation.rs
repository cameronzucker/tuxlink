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

    /// Build a transcript from an existing message slice (multi-turn / resumed
    /// sessions). The caller is responsible for the initial content; no turn is
    /// appended here.
    pub fn from_messages(messages: Vec<Message>) -> Self {
        Self { messages }
    }

    /// Read-only view of the transcript.
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Append a user turn. Used by `run_with_conversation` callers to seed the
    /// new user message before handing the conversation to the loop.
    pub fn push_user(&mut self, text: impl Into<String>) {
        self.messages.push(Message::User(text.into()));
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
    ///
    /// Note: callers must NOT pass a `Cancelled` outcome here — the runner returns
    /// `RunOutcome::Cancelled` before reaching this call on a cancelled tool.
    /// This arm is present to satisfy exhaustive-match under `-D warnings`; it
    /// surfaces the cancellation reason as a tool error in the unlikely event it
    /// is called anyway.
    pub fn push_outcome(&mut self, name: &str, outcome: &ToolOutcome) {
        match outcome {
            ToolOutcome::Ok(value) => self.push_tool_result(name, value.to_string()),
            ToolOutcome::Denied(reason) => {
                self.push_tool_error(name, format!("tool denied: {reason}"))
            }
            ToolOutcome::InvalidArgs(detail) => {
                self.push_tool_error(name, format!("invalid arguments: {detail}"))
            }
            ToolOutcome::Cancelled(c) => self.push_tool_error(name, c),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolOutcome;

    #[test]
    fn tool_outcome_has_cancelled_variant() {
        let _ = ToolOutcome::Cancelled("x".into());
    }

    #[test]
    fn from_messages_roundtrips() {
        let m = vec![Message::User("hi".into()), Message::Assistant("yo".into())];
        assert_eq!(Conversation::from_messages(m.clone()).messages(), m.as_slice());
    }

    #[test]
    fn push_user_appends_user_turn() {
        let mut c = Conversation::from_messages(vec![]);
        c.push_user("hello");
        assert!(matches!(c.messages().last(), Some(Message::User(s)) if s == "hello"));
    }
}
