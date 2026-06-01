//! Management API authentication.
//!
//! When `MANAGEMENT_API_KEY` is set, all `/api/v1/*` routes require the
//! caller to supply the key via either:
//!   - `X-Management-Key: <key>` header, or
//!   - `Authorization: Bearer <key>` header
//!
//! The comparison uses a constant-time equality check (via the `subtle` crate)
//! to prevent timing side-channel attacks that could leak the key byte-by-byte.
//!
//! When the env var is unset the server refuses to start unless the operator
//! has explicitly set `ALLOW_UNAUTH_MANAGEMENT=true`.

use axum::{body::Body, extract::State, http::StatusCode, response::Response};
use subtle::ConstantTimeEq;

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

        let authorized = match provided {
            None => false,
            Some(p) => {
                let p_bytes = p.as_bytes();
                let r_bytes = required.as_bytes();
                // Constant-time comparison prevents byte-by-byte timing oracles.
                // A length mismatch is detected in constant time; the length
                // itself is public (attacker can measure response size anyway).
                p_bytes.len() == r_bytes.len()
                    && bool::from(p_bytes.ct_eq(r_bytes))
            }
        };

        if !authorized {
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
