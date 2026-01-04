use zoey_core::observability::{start_rest_api, CostTracker, RestApiConfig};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env::set_var("OBSERVABILITY_REST_API_ENABLED", "true");
    env::set_var("OBSERVABILITY_REST_API_HOST", "127.0.0.1");
    env::set_var("OBSERVABILITY_REST_API_PORT", "9100");
    let cfg = RestApiConfig::from_env();
    let tracker = CostTracker::new(None);
    start_rest_api(cfg, Some(std::sync::Arc::new(tracker))).await?;
    Ok(())
}
