//! Orbit service proxy handlers (10 tools).

use super::{ProxyContext, ToolProxyRequest, ToolProxyResponse};
use axum::{extract::State, Json};
use tracing::{debug, error};

/// POST /api/orbit/push — Push a branch to orbit from the workspace.
pub async fn orbit_push(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();
    let branch = input["branch"].as_str().unwrap_or_default();
    let force = input["force"].as_bool().unwrap_or(false);

    if org_id.is_empty() || repo.is_empty() || branch.is_empty() {
        return Json(ToolProxyResponse::failure("org_id, repo, and branch are required"));
    }

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let workspace = req
        .context
        .as_ref()
        .map(|c| c.workspace.clone())
        .unwrap_or_default();

    if workspace.is_empty() {
        return Json(ToolProxyResponse::failure("No workspace context provided"));
    }

    let remote_url = format!(
        "https://x-token:{jwt}@{}/{org_id}/{repo}.git",
        ctx.clients
            .orbit_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
    );

    let mut cmd = tokio::process::Command::new("git");
    cmd.arg("push").arg(&remote_url);
    if force {
        cmd.arg("--force");
    }
    cmd.arg(format!("HEAD:refs/heads/{branch}"));
    cmd.current_dir(&workspace);

    let agent_id = req
        .context
        .as_ref()
        .map(|c| c.agent_id.as_str())
        .unwrap_or("");
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    if !agent_id.is_empty() {
        cmd.arg("-c")
            .arg(format!("http.extraHeader=X-Agent-Id: {agent_id}"));
    }

    match cmd.output().await {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            if output.status.success() {
                Json(ToolProxyResponse::success(
                    format!("{stdout}{stderr}").trim(),
                ))
            } else {
                Json(ToolProxyResponse::failure(
                    format!("git push failed: {stderr}").trim(),
                ))
            }
        }
        Err(e) => Json(ToolProxyResponse::failure(format!(
            "Failed to run git push: {e}"
        ))),
    }
}

/// POST /api/orbit/create_repo — Create a new orbit repository.
pub async fn orbit_create_repo(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let _ = &headers;
    let input = &req.input;
    let Some(token) = &ctx.clients.internal_token else {
        return Json(ToolProxyResponse::failure(
            "Internal service token not configured",
        ));
    };

    let body = serde_json::json!({
        "orgId": input["org_id"].as_str().unwrap_or_default(),
        "projectId": input["project_id"].as_str().unwrap_or_default(),
        "ownerId": input["owner_id"].as_str().unwrap_or_default(),
        "name": input["name"].as_str().unwrap_or_default(),
        "visibility": input["visibility"].as_str().unwrap_or("private"),
    });

    let url = format!("{}/internal/repos", ctx.clients.orbit_url);
    proxy_post_internal(&ctx.clients.http, &url, token, &body).await
}

/// POST /api/orbit/list_repos — List accessible repositories.
pub async fn orbit_list_repos(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(_req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!("{}/repos", ctx.clients.orbit_url);
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/orbit/list_branches — List branches in a repository.
pub async fn orbit_list_branches(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!(
        "{}/repos/{org_id}/{repo}/branches",
        ctx.clients.orbit_url
    );
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/orbit/create_branch — Create a new branch.
pub async fn orbit_create_branch(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "name": input["name"].as_str().unwrap_or_default(),
        "source": input["source"].as_str().unwrap_or("main"),
    });

    let url = format!(
        "{}/repos/{org_id}/{repo}/branches",
        ctx.clients.orbit_url
    );
    proxy_post_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

/// POST /api/orbit/list_commits — List recent commits.
pub async fn orbit_list_commits(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();
    let git_ref = input["ref"].as_str().unwrap_or_default();
    let limit = input["limit"].as_u64().unwrap_or(20);

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let mut url = format!(
        "{}/repos/{org_id}/{repo}/commits?limit={limit}",
        ctx.clients.orbit_url
    );
    if !git_ref.is_empty() {
        url.push_str(&format!("&ref={git_ref}"));
    }
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/orbit/get_diff — Get diff for a commit.
pub async fn orbit_get_diff(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();
    let sha = input["sha"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let url = format!(
        "{}/repos/{org_id}/{repo}/commits/{sha}/diff",
        ctx.clients.orbit_url
    );
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/orbit/create_pr — Open a pull request.
pub async fn orbit_create_pr(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "sourceBranch": input["source_branch"].as_str().unwrap_or_default(),
        "targetBranch": input["target_branch"].as_str().unwrap_or_default(),
        "title": input["title"].as_str().unwrap_or_default(),
        "description": input["description"].as_str().unwrap_or_default(),
    });

    let url = format!(
        "{}/repos/{org_id}/{repo}/pulls",
        ctx.clients.orbit_url
    );
    proxy_post_jwt(&ctx.clients.http, &url, &jwt, &body).await
}

/// POST /api/orbit/list_prs — List pull requests.
pub async fn orbit_list_prs(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();
    let status = input["status"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let mut url = format!(
        "{}/repos/{org_id}/{repo}/pulls",
        ctx.clients.orbit_url
    );
    if !status.is_empty() {
        url.push_str(&format!("?status={status}"));
    }
    proxy_get_jwt(&ctx.clients.http, &url, &jwt).await
}

/// POST /api/orbit/merge_pr — Merge a pull request.
pub async fn orbit_merge_pr(
    State(ctx): State<ProxyContext>,
    headers: axum::http::HeaderMap,
    Json(req): Json<ToolProxyRequest>,
) -> Json<ToolProxyResponse> {
    let input = &req.input;
    let org_id = input["org_id"].as_str().unwrap_or_default();
    let repo = input["repo"].as_str().unwrap_or_default();
    let pr_id = input["pr_id"].as_str().unwrap_or_default();

    let jwt = match super::extract_jwt(&headers) {
        Some(t) => t,
        None => return Json(ToolProxyResponse::failure("No session JWT available")),
    };

    let body = serde_json::json!({
        "strategy": input["strategy"].as_str().unwrap_or("merge"),
    });

    let url = format!(
        "{}/repos/{org_id}/{repo}/pulls/{pr_id}/merge",
        ctx.clients.orbit_url
    );
    proxy_post_jwt(&ctx.clients.http, &url, &jwt, &body).await
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
                Json(ToolProxyResponse::failure(format!(
                    "HTTP {status}: {truncated}"
                )))
            }
        }
        Err(e) => Json(ToolProxyResponse::failure(format!(
            "Failed to read response: {e}"
        ))),
    }
}
