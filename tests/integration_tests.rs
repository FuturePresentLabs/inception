use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;

use inception_registry::test_helpers::{create_test_app, extract_json};



#[tokio::test]
async fn test_create_session_endpoint() {
    let app: axum::Router = create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/sessions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"agent_type": "claude_code", "capabilities": ["rust", "python"]}"#,
        ))
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json["id"].as_str().unwrap().starts_with("sess-"));
    assert_eq!(json["status"], "spawning");
    assert!(json["websocket_url"].as_str().unwrap().contains("/ws"));
}

#[tokio::test]
async fn test_get_session_endpoint() {
    let app: axum::Router = create_test_app().await;

    // First create a session
    let create_request = Request::builder()
        .method("POST")
        .uri("/v1/sessions")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"agent_type": "claude_code"}"#))
        .unwrap();

    let create_response = app.clone().oneshot(create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_id = json["id"].as_str().unwrap();

    // Now get the session
    let get_request = Request::builder()
        .uri(format!("/v1/sessions/{}", session_id))
        .body(Body::empty())
        .unwrap();

    let get_response = app.oneshot(get_request).await.unwrap();
    assert_eq!(get_response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["id"], session_id);
    assert_eq!(json["agent_type"], "claude_code");
}

#[tokio::test]
async fn test_get_nonexistent_session() {
    let app: axum::Router = create_test_app().await;

    let request = Request::builder()
        .uri("/v1/sessions/sess-nonexistent")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_list_sessions_endpoint() {
    let app: axum::Router = create_test_app().await;

    // Create multiple sessions
    for _ in 0..3 {
        let request = Request::builder()
            .method("POST")
            .uri("/v1/sessions")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"agent_type": "claude_code"}"#))
            .unwrap();

        app.clone().oneshot(request).await.unwrap();
    }

    // List sessions
    let request = Request::builder()
        .uri("/v1/sessions")
        .body(Body::empty())
        .unwrap();

    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    assert!(json.as_array().unwrap().len() >= 3);
}

#[tokio::test]
async fn test_send_message_endpoint() {
    let app: axum::Router = create_test_app().await;

    // Create a session
    let create_request = Request::builder()
        .method("POST")
        .uri("/v1/sessions")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"agent_type": "claude_code"}"#))
        .unwrap();

    let create_response = app.clone().oneshot(create_request).await.unwrap();
    let body = axum::body::to_bytes(create_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_id = json["id"].as_str().unwrap();

    // Send a message (currently stubbed, returns ACCEPTED)
    let message_request = Request::builder()
        .method("POST")
        .uri(format!("/v1/sessions/{}/messages", session_id))
        .header("content-type", "application/json")
        .body(Body::from(r#"{"id": "msg-123", "content": "Hello, world!", "timestamp": "2026-03-22T12:00:00Z"}"#))
        .unwrap();

    let message_response = app.oneshot(message_request).await.unwrap();
    assert_eq!(message_response.status(), StatusCode::ACCEPTED);
}

#[tokio::test]
async fn test_session_capabilities_preserved() {
    let app: axum::Router = create_test_app().await;

    let request = Request::builder()
        .method("POST")
        .uri("/v1/sessions")
        .header("content-type", "application/json")
        .body(Body::from(
            r#"{"agent_type": "claude_code", "capabilities": ["rust", "python", "typescript"], "metadata": {"key": "value"}}"#,
        ))
        .unwrap();

    let response = app.clone().oneshot(request).await.unwrap();
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let session_id = json["id"].as_str().unwrap();

    // Get session and verify capabilities
    let get_request = Request::builder()
        .uri(format!("/v1/sessions/{}", session_id))
        .body(Body::empty())
        .unwrap();

    let get_response = app.oneshot(get_request).await.unwrap();
    let body = axum::body::to_bytes(get_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    let capabilities = json["capabilities"].as_array().unwrap();
    assert!(capabilities.contains(&serde_json::json!("rust")));
    assert!(capabilities.contains(&serde_json::json!("python")));
    assert!(capabilities.contains(&serde_json::json!("typescript")));

    let metadata = json["metadata"].as_object().unwrap();
    assert_eq!(metadata["key"], "value");
}
