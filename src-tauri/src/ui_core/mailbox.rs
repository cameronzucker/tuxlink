//! Transport-agnostic mailbox core functions.
//!
//! `mailbox_list` (the Tauri command) is now a thin adapter over
//! `list_mailbox` here. Consumed by the `mailbox_list` Tauri command (Plan 1)
//! and later by the MCP `mailbox_list` tool (Plan 3).

use std::sync::Arc;
use crate::ui_commands::{MessageMetaDto, UiError};
use crate::native_mailbox::FolderRef;
use crate::winlink_backend::WinlinkBackend;

/// List the messages in `folder` using the given backend.
///
/// Takes an already-parsed [`FolderRef`] (a domain type): folder-string
/// parsing is an adapter concern, kept in the caller, so the original
/// error ordering — folder validated before backend state — is preserved
/// and the core works on domain types, not wire strings. Returns the same
/// DTO shape the `mailbox_list` Tauri command always returned.
pub async fn list_mailbox(
    backend: &Arc<dyn WinlinkBackend>,
    folder: FolderRef,
) -> Result<Vec<MessageMetaDto>, UiError> {
    let metas = match folder {
        FolderRef::System(f) => backend.list_messages(f).await?,
        FolderRef::User(slug) => backend.list_user_messages(&slug).await?,
    };
    Ok(metas.into_iter().map(MessageMetaDto::from).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_mailbox::{FolderRef, Mailbox};
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

        let metas = list_mailbox(&backend, FolderRef::System(MailboxFolder::Inbox))
            .await
            .unwrap();
        assert_eq!(metas.len(), 1);
        assert_eq!(metas[0].subject, "Hi");
    }
}
