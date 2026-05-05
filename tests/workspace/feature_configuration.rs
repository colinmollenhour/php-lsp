//! workspace/didChangeConfiguration: PHP version detection, validation,
//! semantic-token refresh, and repeated calls.

use super::*;

use expect_test::expect;
use serde_json::json;

fn extract_log_message(notif: &serde_json::Value) -> String {
    notif["params"]["message"].as_str().unwrap_or("").to_owned()
}

#[tokio::test]
async fn change_configuration_valid_php_version_is_logged() {
    let mut server = TestServer::new().await;
    let log = server
        .change_configuration(json!({ "phpVersion": "8.3" }))
        .await;
    let msg = extract_log_message(&log);
    expect!["php-lsp: using PHP 8.3 (set by editor)"].assert_eq(&msg);
}

#[tokio::test]
async fn change_configuration_invalid_php_version_logs_warning() {
    let mut server = TestServer::new().await;

    server
        .client()
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": null }),
        )
        .await;
    let (req_id, _) = server
        .client()
        .expect_server_request("workspace/configuration")
        .await;
    server
        .client()
        .reply_to_server_request(req_id, json!([{ "phpVersion": "5.6" }]))
        .await;

    let warning_msg = server.client().read_notification("window/logMessage").await;
    let warning_text = extract_log_message(&warning_msg);
    expect![[
        r#"php-lsp: unsupported phpVersion "5.6" — valid values: 7.4, 8.0, 8.1, 8.2, 8.3, 8.4, 8.5"#
    ]]
    .assert_eq(&warning_text);

    let info_msg = server.client().read_notification("window/logMessage").await;
    let info_text = extract_log_message(&info_msg);
    assert!(
        info_text.starts_with("php-lsp: using PHP "),
        "expected PHP version log: {info_text:?}"
    );
}

#[tokio::test]
async fn change_configuration_triggers_semantic_token_refresh() {
    let mut server = TestServer::new().await;

    server
        .client()
        .notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": null }),
        )
        .await;
    let (req_id, _) = server
        .client()
        .expect_server_request("workspace/configuration")
        .await;
    server
        .client()
        .reply_to_server_request(req_id, json!([{ "phpVersion": "8.1" }]))
        .await;

    let _log = server.client().read_notification("window/logMessage").await;

    let (refresh_id, _) = server
        .client()
        .expect_server_request("workspace/semanticTokens/refresh")
        .await;
    server
        .client()
        .reply_to_server_request(refresh_id, json!(null))
        .await;
}

#[tokio::test]
async fn change_configuration_can_be_called_twice() {
    let mut server = TestServer::new().await;

    let log1 = server
        .change_configuration(json!({ "phpVersion": "8.1" }))
        .await;
    let msg1 = extract_log_message(&log1);
    expect!["php-lsp: using PHP 8.1 (set by editor)"].assert_eq(&msg1);

    let log2 = server
        .change_configuration(json!({ "phpVersion": "8.3" }))
        .await;
    let msg2 = extract_log_message(&log2);
    expect!["php-lsp: using PHP 8.3 (set by editor)"].assert_eq(&msg2);
}

#[tokio::test]
async fn change_configuration_empty_config_uses_detected_version() {
    let mut server = TestServer::new().await;

    let log = server.change_configuration(json!({})).await;
    let msg = extract_log_message(&log);
    assert!(
        msg.starts_with("php-lsp: using PHP "),
        "expected version log: {msg:?}"
    );
    assert!(
        !msg.contains("set by editor"),
        "empty config must not claim 'set by editor': {msg:?}"
    );
}
