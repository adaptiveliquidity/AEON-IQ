//! Management API authentication.
//!
//! When `MANAGEMENT_API_KEY` is set in the environment, all `/api/v1/*`
//! routes require the caller to supply the key via either:
//!   - `X-Management-Key: <key>` header, or
//!   - `Authorization: Bearer <key>` header
//!
//! If the env var is unset the middleware is a no-op, allowing local dev
//! without configuration overhead.

use axum::{body::Body, extract::State, http::StatusCode, response::Response};

pub async fn check_management_key(
    State(state): State<crate::AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if let Some(ref required) = state.config.management_api_key {
        let provided = req
            .headers()
            .get("x-management-key")
            .or_else(|| req.headers().get("authorization"))
            .and_then(|v| v.to_str().ok())
            .map(|v| v.trim_start_matches("Bearer ").trim());

        if provided != Some(required.as_str()) {
            return Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"error":"Unauthorized. Provide the management key via X-Management-Key or Authorization: Bearer <key>."}"#,
                ))
                .unwrap();
        }
    }
    next.run(req).await
}
