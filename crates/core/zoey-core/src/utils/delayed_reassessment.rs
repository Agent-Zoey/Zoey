use crate::types::*;

pub struct DelayedReassessment;

impl DelayedReassessment {
    pub fn enabled(rt: &crate::AgentRuntime) -> bool {
        rt.get_setting("AUTONOMOUS_DELAYED_REASSESSMENT")
            .and_then(|v| v.as_bool())
            .unwrap_or_else(|| {
                rt.get_setting("ui:delayed_reassessment")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            })
    }

    pub fn start(rt: &mut crate::AgentRuntime, room_id: UUID, text: &str) {
        let key_t = format!("delayed:{}:ts", room_id);
        let key_p = format!("delayed:{}:pending", room_id);
        rt.set_setting(
            &key_t,
            serde_json::json!(chrono::Utc::now().timestamp()),
            false,
        );
        rt.set_setting(&key_p, serde_json::json!(text), false);
    }

    pub fn clear(rt: &mut crate::AgentRuntime, room_id: UUID) {
        let key_t = format!("delayed:{}:ts", room_id);
        let key_p = format!("delayed:{}:pending", room_id);
        rt.set_setting(&key_t, serde_json::Value::Null, false);
        rt.set_setting(&key_p, serde_json::Value::Null, false);
    }

    pub fn pending(rt: &crate::AgentRuntime, room_id: UUID) -> Option<(i64, String)> {
        let key_t = format!("delayed:{}:ts", room_id);
        let key_p = format!("delayed:{}:pending", room_id);
        let ts = rt.get_setting(&key_t).and_then(|v| v.as_i64())?;
        let text = rt
            .get_setting(&key_p)
            .and_then(|v| v.as_str().map(|s| s.to_string()))?;
        Some((ts, text))
    }

    pub fn should_wait(prev_ts: i64) -> bool {
        let now = chrono::Utc::now().timestamp();
        now - prev_ts <= 2
    }

    pub fn merge(a: &str, b: &str) -> String {
        if a.is_empty() {
            return b.to_string();
        }
        if b.is_empty() {
            return a.to_string();
        }
        format!(
            "{} {}",
            a.trim_end_matches(|c: char| c == '?' || c == '.' || c == '!' || c == '-' || c == ' '),
            b.trim()
        )
    }
}
