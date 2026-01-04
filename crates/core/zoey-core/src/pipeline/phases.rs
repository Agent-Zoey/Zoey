use super::mps::{mp1_language, mp2_classify, mp3_topics_keywords, mp4_entities_simple};
use crate::pipeline::{profiler::summarize_profiles, types::*};
use crate::preprocessor::Phase0Preprocessor;
use crate::types::*;
use std::sync::{Arc, RwLock};

pub async fn execute_phase0(
    runtime: Arc<RwLock<crate::AgentRuntime>>,
    message: &Memory,
) -> crate::Result<PhaseProfile> {
    let pre = Phase0Preprocessor::new(runtime.clone());
    let start = std::time::Instant::now();
    let _ = pre.execute(message).await?;
    let total = start.elapsed().as_millis() as i64;
    let entries: Vec<(String, i64)> = vec![]; // already summarized inside preprocessor
    summarize_profiles("phase0", &entries, total);
    Ok(PhaseProfile {
        phase: "phase0",
        total_ms: total,
        bottlenecks: entries,
    })
}

pub async fn execute_phase1(
    runtime: Arc<RwLock<crate::AgentRuntime>>,
    message: &Memory,
) -> crate::Result<PhaseProfile> {
    let start = std::time::Instant::now();
    let mut bottlenecks = Vec::new();
    let text = message.content.text.clone();

    let t0 = std::time::Instant::now();
    let lang = mp1_language(&text);
    bottlenecks.push(("P1:language".to_string(), t0.elapsed().as_millis() as i64));
    {
        let mut r = runtime.write().unwrap();
        r.set_setting("phase1:language", serde_json::json!(lang), false);
    }

    let t1 = std::time::Instant::now();
    let (intent, sentiment, tone) = mp2_classify(&text);
    bottlenecks.push((
        "P1:classification".to_string(),
        t1.elapsed().as_millis() as i64,
    ));
    {
        let mut r = runtime.write().unwrap();
        r.set_setting("phase1:intent", serde_json::json!(intent), false);
        r.set_setting("phase1:sentiment", serde_json::json!(sentiment), false);
        r.set_setting("phase1:tone", serde_json::json!(tone), false);
    }

    let total = start.elapsed().as_millis() as i64;
    summarize_profiles("phase1", &bottlenecks, total);
    Ok(PhaseProfile {
        phase: "phase1",
        total_ms: total,
        bottlenecks,
    })
}

pub async fn execute_phase2(
    runtime: Arc<RwLock<crate::AgentRuntime>>,
    message: &Memory,
) -> crate::Result<PhaseProfile> {
    let start = std::time::Instant::now();
    let mut bottlenecks = Vec::new();
    let text = message.content.text.clone();

    let t0 = std::time::Instant::now();
    let (topics, keywords) = mp3_topics_keywords(&text);
    bottlenecks.push((
        "P2:topics_keywords".to_string(),
        t0.elapsed().as_millis() as i64,
    ));
    {
        let mut r = runtime.write().unwrap();
        r.set_setting("phase2:topics", serde_json::json!(topics), false);
        r.set_setting("phase2:keywords", serde_json::json!(keywords), false);
    }

    let t1 = std::time::Instant::now();
    let ents = mp4_entities_simple(&text);
    bottlenecks.push(("P2:entities".to_string(), t1.elapsed().as_millis() as i64));
    {
        let mut r = runtime.write().unwrap();
        r.set_setting("phase2:entities", serde_json::json!(ents), false);
    }

    let total = start.elapsed().as_millis() as i64;
    summarize_profiles("phase2", &bottlenecks, total);
    Ok(PhaseProfile {
        phase: "phase2",
        total_ms: total,
        bottlenecks,
    })
}
