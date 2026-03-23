//! Aura Network service proxy handlers (5 tools).

use super::{ProxyContext, ToolProxyRequest, ToolProxyResponse};
use axum::{extract::State, Json};
use tracing::{debug, error};

/// POST /api/network/post_to_feed (uses X-Internal-Token)
pub async fn post_to_feed(
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
        "profileId": input["profile_id"].as_str().unwrap_or_default(),
        "title": input["title"].as_str().unwrap_or_default(),
        "summary": input["summary"].as_str(),
        "postType": input["post_type"].as_str().unwrap_or("post"),
        "agentId": input["agent_id"].as_str(),
        "userId": input["user_id"].as_str(),
        "metadata": input["metadata"],
    });

    let url = format!("{}/internal/posts", ctx.clients.aura_network_url);
    proxy_post_internal(&ctx.clients.http, &url, token, &body).await
}

/// POST /api/network/list_projects (uses JWT)
pub async fn list_projects(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!("{}/api/projects?org_id={org_id}", ctx.clients.aura_network_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/network/get_project (uses JWT)
pub async fn get_project(
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

    let url = format!("{}/api/projects/{project_id}", ctx.clients.aura_network_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/network/check_budget (uses X-Internal-Token)
pub async fn check_budget(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let _ = &headers;
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let user_id = input["user_id"].as_str().unwrap_or_default();

    let Some(token) = &ctx.clients.internal_token else {
        return Json(ToolProxyResponse::failure("Internal service token not configured"));
    };

    let url = format!(
        "{}/internal/orgs/{org_id}/members/{user_id}/budget",
        ctx.clients.aura_network_url
    );
    proxy_get_internal(&ctx.clients.http, &url, token).await
}

/// POST /api/network/record_usage (uses X-Internal-Token)
pub async fn record_usage(
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
        "orgId": input["org_id"].as_str().unwrap_or_default(),
        "userId": input["user_id"].as_str().unwrap_or_default(),
        "inputTokens": input["input_tokens"].as_u64().unwrap_or(0),
        "outputTokens": input["output_tokens"].as_u64().unwrap_or(0),
        "agentId": input["agent_id"].as_str(),
        "model": input["model"].as_str(),
    });

    let url = format!("{}/internal/usage", ctx.clients.aura_network_url);
    proxy_post_internal(&ctx.clients.http, &url, token, &body).await
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

async fn proxy_get_internal(
    http: &reqwest::Client,
    url: &str,
    token: &str,
) -> Json<ToolProxyResponse> {
    debug!(url = %url, "Proxy GET (internal token)");
    match http
        .get(url)
        .header("X-Internal-Token", token)
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
