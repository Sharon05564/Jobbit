use axum::Json;
use serde_json::{json, Value};

/// GET /health
///
/// A simple liveness check so load balancers and Docker health checks can
/// verify the server is up and responding.
pub async fn health_check() -> Json<Value> {
    Json(json!({
        "status": "ok",
        "service": "jobbit"
    }))
}
