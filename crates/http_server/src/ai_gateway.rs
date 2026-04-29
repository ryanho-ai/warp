use std::{env, sync::Arc};

use axum::{
    Json, Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AiGatewayState {
    http: reqwest::Client,
    openai_api_key: Option<String>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
    openai_key_configured: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<serde_json::Value>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

pub fn router() -> Router {
    let state = AiGatewayState {
        http: reqwest::Client::new(),
        openai_api_key: env::var("OPENAI_API_KEY").ok(),
    };

    Router::new()
        .route("/ai/health", get(health))
        .route("/ai/openai/chat/completions", post(chat_completions))
        .with_state(Arc::new(state))
}

async fn health(State(state): State<Arc<AiGatewayState>>) -> impl IntoResponse {
    Json(HealthResponse {
        ok: true,
        openai_key_configured: state.openai_api_key.is_some(),
    })
}

async fn chat_completions(
    State(state): State<Arc<AiGatewayState>>,
    Json(payload): Json<ChatCompletionsRequest>,
) -> impl IntoResponse {
    let Some(api_key) = &state.openai_api_key else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "OPENAI_API_KEY is not set"})),
        )
            .into_response();
    };

    let mut body = serde_json::Map::new();
    body.insert(
        "model".to_string(),
        serde_json::Value::String(payload.model),
    );
    body.insert(
        "messages".to_string(),
        serde_json::Value::Array(payload.messages),
    );
    body.extend(payload.extra);

    let response = state
        .http
        .post("https://api.openai.com/v1/chat/completions")
        .header("authorization", format!("Bearer {api_key}"))
        .header("content-type", "application/json")
        .json(&body)
        .send()
        .await;

    let Ok(response) = response else {
        return (
            StatusCode::BAD_GATEWAY,
            Json(serde_json::json!({"error": "Failed to reach OpenAI"})),
        )
            .into_response();
    };

    let status = response.status();
    let headers = response.headers().clone();
    let json = response
        .json::<serde_json::Value>()
        .await
        .unwrap_or_else(|_| serde_json::json!({"error": "Invalid JSON from OpenAI"}));

    let mut out_headers = HeaderMap::new();
    if let Some(ct) = headers.get("content-type") {
        out_headers.insert("content-type", ct.clone());
    }

    (status, out_headers, Json(json)).into_response()
}
