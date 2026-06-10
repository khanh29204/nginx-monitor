use axum::http::{HeaderMap, StatusCode};
use reqwest::Client;
use serde::Deserialize;
use tracing::warn;

#[derive(Deserialize)]
pub struct TokenQuery {
    pub token: Option<String>,
    pub interval: Option<u64>,
}

pub async fn verify_token(
    headers: &HeaderMap,
    query_token: Option<&str>,
    domain_auth: &str,
    http: &Client,
) -> Result<(), StatusCode> {
    // Extract token from Authorization header or query param
    let token = if let Some(auth) = headers.get("authorization") {
        let val = auth.to_str().unwrap_or("");
        if val.starts_with("Bearer ") {
            val[7..].to_string()
        } else {
            val.to_string()
        }
    } else if let Some(t) = query_token {
        t.to_string()
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if token.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    #[derive(serde::Deserialize)]
    struct AuthResponse {
        id: Option<serde_json::Value>,
    }

    match http
        .post(domain_auth)
        .json(&serde_json::json!({ "token": token }))
        .send()
        .await
    {
        Ok(resp) if resp.status().is_success() => {
            match resp.json::<AuthResponse>().await {
                Ok(body) if body.id.is_some() => Ok(()),
                _ => {
                    warn!("Auth response missing id");
                    Err(StatusCode::UNAUTHORIZED)
                }
            }
        }
        Ok(resp) => {
            warn!("Auth failed with status: {}", resp.status());
            Err(StatusCode::UNAUTHORIZED)
        }
        Err(e) => {
            warn!("Auth request error: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}