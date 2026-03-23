//! Aura Storage service proxy handlers (12 tools).

use super::{ProxyContext, ToolProxyRequest, ToolProxyResponse};
use axum::{extract::State, Json};
use tracing::{debug, error};

// === Tasks ===

/// POST /api/storage/create_task
pub async fn create_task(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let project_id = input["project_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "title": input["title"].as_str().unwrap_or_default(),
        "description": input["description"].as_str(),
        "specId": input["spec_id"].as_str(),
        "orderIndex": input["order_index"].as_u64(),
    });

    let url = format!("{}/api/projects/{project_id}/tasks", ctx.clients.aura_storage_url);
    proxy_post_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

/// POST /api/storage/list_tasks
pub async fn list_tasks(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let project_id = input["project_id"].as_str().unwrap_or_default();
    let status = input["status"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let mut url = format!("{}/api/projects/{project_id}/tasks", ctx.clients.aura_storage_url);
    if !status.is_empty() {
        url.push_str(&format!("?status={status}"));
    }
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/storage/get_task
pub async fn get_task(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let task_id = input["task_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!("{}/api/tasks/{task_id}", ctx.clients.aura_storage_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/storage/update_task
pub async fn update_task(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let task_id = input["task_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "title": input["title"].as_str(),
        "description": input["description"].as_str(),
        "executionNotes": input["execution_notes"].as_str(),
        "filesChanged": input["files_changed"],
    });

    let url = format!("{}/api/tasks/{task_id}", ctx.clients.aura_storage_url);
    proxy_put_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

/// POST /api/storage/transition_task
pub async fn transition_task(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let task_id = input["task_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "status": input["status"].as_str().unwrap_or_default(),
    });

    let url = format!("{}/api/tasks/{task_id}/transition", ctx.clients.aura_storage_url);
    proxy_post_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

// === Specs ===

/// POST /api/storage/create_spec
pub async fn create_spec(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let project_id = input["project_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "title": input["title"].as_str().unwrap_or_default(),
        "markdownContents": input["markdown_contents"].as_str().unwrap_or_default(),
        "orderIndex": input["order_index"].as_u64(),
    });

    let url = format!("{}/api/projects/{project_id}/specs", ctx.clients.aura_storage_url);
    proxy_post_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

/// POST /api/storage/list_specs
pub async fn list_specs(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let project_id = input["project_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!("{}/api/projects/{project_id}/specs", ctx.clients.aura_storage_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/storage/get_spec
pub async fn get_spec(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let spec_id = input["spec_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!("{}/api/specs/{spec_id}", ctx.clients.aura_storage_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/storage/update_spec
pub async fn update_spec(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let spec_id = input["spec_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "title": input["title"].as_str(),
        "markdownContents": input["markdown_contents"].as_str(),
        "orderIndex": input["order_index"].as_u64(),
    });

    let url = format!("{}/api/specs/{spec_id}", ctx.clients.aura_storage_url);
    proxy_put_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

// === Logging & Stats ===

/// POST /api/storage/create_log (uses X-Internal-Token)
pub async fn create_log(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let _ = &headers;
    let input = &req.input;
    let Some(token) = &ctx.clients.internal_token else {
        return Json(ToolProxyResponse::failure("Internal service token not configured"));
    };

    let body = serde_json::json!({
        "projectId": input["project_id"].as_str().unwrap_or_default(),
        "message": input["message"].as_str().unwrap_or_default(),
        "level": input["level"].as_str().unwrap_or("info"),
        "projectAgentId": input["project_agent_id"].as_str(),
        "metadata": input["metadata"],
    });

    let url = format!("{}/internal/logs", ctx.clients.aura_storage_url);
    proxy_post_internal(&ctx.clients.http, &url, token, &body).await
}

/// POST /api/storage/list_logs
pub async fn list_logs(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let project_id = input["project_id"].as_str().unwrap_or_default();
    let level = input["level"].as_str().unwrap_or_default();
    let limit = input["limit"].as_u64().unwrap_or(50);

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let mut url = format!("{}/api/projects/{project_id}/logs?limit={limit}", ctx.clients.aura_storage_url);
    if !level.is_empty() {
        url.push_str(&format!("&level={level}"));
    }
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/storage/get_project_stats
pub async fn get_project_stats(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let project_id = input["project_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!("{}/api/stats?scope=project&projectId={project_id}", ctx.clients.aura_storage_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

// === Shared helpers ===

async fn proxy_get_jwt(
    http: &reqwest::Client,
    url: &str,
    jwt: &str,
) -> Json<ToolProxyResponse> {
    debug!(url = %url, "Proxy GET (JWT)");
    match http.get(url).bearer_auth(jwt).send().await {
        Ok(resp) => parse_proxy_response(resp).await,
        Err(e) => {
            error!(url = %url, error = %e, "Proxy request failed");
            Json(ToolProxyResponse::failure(format!("Request failed: {e}")))
        }
    }
}

async fn proxy_post_jwt(
    http: &reqwest::Client,
    url: &str,
    jwt: &str,
    body: &serde_json::Value,
) -> Json<ToolProxyResponse> {
    debug!(url = %url, "Proxy POST (JWT)");
    match http.post(url).bearer_auth(jwt).json(body).send().await {
        Ok(resp) => parse_proxy_response(resp).await,
        Err(e) => {
            error!(url = %url, error = %e, "Proxy request failed");
            Json(ToolProxyResponse::failure(format!("Request failed: {e}")))
        }
    }
}

async fn proxy_put_jwt(
    http: &reqwest::Client,
    url: &str,
    jwt: &str,
    body: &serde_json::Value,
) -> Json<ToolProxyResponse> {
    debug!(url = %url, "Proxy PUT (JWT)");
    match http.put(url).bearer_auth(jwt).json(body).send().await {
        Ok(resp) => parse_proxy_response(resp).await,
        Err(e) => {
            error!(url = %url, error = %e, "Proxy request failed");
            Json(ToolProxyResponse::failure(format!("Request failed: {e}")))
        }
    }
}

async fn proxy_post_internal(
    http: &reqwest::Client,
    url: &str,
    token: &str,
    body: &serde_json::Value,
) -> Json<ToolProxyResponse> {
    debug!(url = %url, "Proxy POST (internal token)");
    match http
        .post(url)
        .header("X-Internal-Token", token)
        .json(body)
        .send()
        .await
    {
        Ok(resp) => parse_proxy_response(resp).await,
        Err(e) => {
            error!(url = %url, error = %e, "Proxy request failed");
            Json(ToolProxyResponse::failure(format!("Request failed: {e}")))
        }
    }
}

async fn parse_proxy_response(resp: reqwest::Response) -> Json<ToolProxyResponse> {
    let status = resp.status();
    match resp.text().await {
        Ok(body) => {
            if status.is_success() {
                Json(ToolProxyResponse::success(body))
            } else {
                let truncated: String = body.chars().take(500).collect();
                Json(ToolProxyResponse::failure(format!("HTTP {status}: {truncated}")))
            }
        }
        Err(e) => Json(ToolProxyResponse::failure(format!("Failed to read response: {e}"))),
    }
}
