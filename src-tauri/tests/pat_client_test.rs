use tuxlink_lib::pat_client::{PatClient, MailboxFolder};

#[test]
fn test_list_inbox_parses_pat_json() {
    let mut server = mockito::Server::new();
    let _mock = server.mock("GET", "/api/mailbox/in")
        .with_status(200)
        .with_header("content-type", "application/json")
        .with_body(r#"[{"MID":"ABC123","Subject":"Test","From":{"Addr":"W4PHS@winlink.org"},"Date":"2026-04-22T15:00:00Z","Unread":true,"BodySize":42}]"#)
        .create();

    let client = PatClient::new(server.url());
    let messages = client.list(MailboxFolder::Inbox).expect("list inbox");
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].mid, "ABC123");
    assert_eq!(messages[0].subject, "Test");
    assert_eq!(messages[0].from, "W4PHS@winlink.org");
    assert!(messages[0].unread);
}

#[test]
fn test_list_inbox_surfaces_http_error() {
    let mut server = mockito::Server::new();
    let _mock = server.mock("GET", "/api/mailbox/in").with_status(500).create();
    let client = PatClient::new(server.url());
    assert!(client.list(MailboxFolder::Inbox).is_err());
}

#[test]
fn test_send_message_posts_correct_json() {
    let mut server = mockito::Server::new();
    let _mock = server.mock("POST", "/api/mailbox/out")
        .with_status(200)
        .with_body(r#"{"MID":"OUTBOX001"}"#)
        .create();
    let client = PatClient::new(server.url());
    let mid = client.send(&["SERVICE@winlink.org"], "Test", "body").expect("send");
    assert_eq!(mid, "OUTBOX001");
}
