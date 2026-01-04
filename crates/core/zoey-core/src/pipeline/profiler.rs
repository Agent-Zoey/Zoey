use tracing::debug;

pub fn summarize_profiles(tag: &str, entries: &[(String, i64)], total_ms: i64) {
    let mut sorted = entries.to_vec();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    let top: Vec<String> = sorted
        .iter()
        .take(3)
        .map(|(k, d)| format!("{}:{}ms", k, d))
        .collect();
    debug!(phase = tag, total_ms, top = ?top, "pipeline_profiles");
}
