//! Transport-agnostic mailbox core functions.
//!
//! `mailbox_list` (the Tauri command) is now a thin adapter over
//! `list_mailbox` here. Consumed by the `mailbox_list` Tauri command (Plan 1)
//! and later by the MCP `mailbox_list` tool (Plan 3).

use std::sync::Arc;
use crate::ui_commands::{MessageMetaDto, UiError};
use crate::winlink_backend::WinlinkBackend;

/// List the messages in `folder` using the given backend.
///
/// `folder` is parsed via [`crate::ui_commands::parse_folder_ref`] before
/// dispatching to the backend, preserving the same routing logic as the
/// `mailbox_list` Tauri command. Returns the same DTO shape so the adapter
/// shim in `ui_commands` is byte-identical to the old inline body.
pub async fn list_mailbox(
    backend: &Arc<dyn WinlinkBackend>,
    folder: &str,
) -> Result<Vec<MessageMetaDto>, UiError> {
    use crate::native_mailbox::FolderRef;
    let parsed = crate::ui_commands::parse_folder_ref(folder)?;
    let metas = match parsed {
        FolderRef::System(f) => backend.list_messages(f).await?,
        FolderRef::User(slug) => backend.list_user_messages(&slug).await?,
    };
    Ok(metas.into_iter().map(MessageMetaDto::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_mailbox::Mailbox;
    use crate::winlink_backend::{MailboxFolder, MessageId, NativeBackend};
    use crate::test_helpers::native_test_config;
    use crate::winlink::compose::compose_message;
    use tempfile::tempdir;

    // Seeds one inbox message and lists it through the extracted core fn.
    #[tokio::test]
    async fn list_mailbox_returns_seeded_inbox_message() {
        let dir = tempdir().unwrap();
        // Seed a raw message directly into the mailbox the backend will read.
        let seed = Mailbox::new(dir.path());
        let raw = compose_message(
            "N7CPZ", &["W1AW"], &[], "Hi", "body", 1_716_200_000,
        ).to_bytes();
        let _id: MessageId = seed.store(MailboxFolder::Inbox, &raw).unwrap();

        let backend: Arc<dyn WinlinkBackend> =
            Arc::new(NativeBackend::new(native_test_config(), dir.path()));

        let metas = list_mailbox(&backend, "inbox").await.unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].subject, "Hi");
    }
}
