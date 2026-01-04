use crate::detectors::analyze_all;
use crate::nlp::{double_metaphone, normalized_similarity};
use crate::pipeline::mps::*;
use crate::planner::complexity::ComplexityAssessment;
use crate::types::*;
use crate::Result;
use std::sync::{Arc, RwLock};
use tokio::task::JoinSet;
use tracing::{debug, info};

#[derive(Clone)]
pub struct Phase0Output {
    pub language: Option<String>,
    pub intent: Option<String>,
    pub sentiment: Option<String>,
    pub tone: Option<String>,
    pub topics: Vec<String>,
    pub keywords: Vec<String>,
    pub entities: Vec<String>,
    pub complexity: Option<ComplexityAssessment>,
    pub agent_candidates: Vec<String>,
    pub available_actions: Vec<String>,
}

impl Phase0Output {
    pub fn new() -> Self {
        Self {
            language: None,
            intent: None,
            sentiment: None,
            tone: None,
            topics: Vec::new(),
            keywords: Vec::new(),
            entities: Vec::new(),
            complexity: None,
            agent_candidates: Vec::new(),
            available_actions: Vec::new(),
        }
    }
}

pub struct Phase0Preprocessor {
    runtime: Arc<RwLock<crate::AgentRuntime>>,
}

impl Phase0Preprocessor {
    pub fn new(runtime: Arc<RwLock<crate::AgentRuntime>>) -> Self {
        Self { runtime }
    }

    pub async fn execute(&self, message: &Memory) -> Result<Phase0Output> {
        let mut js: JoinSet<(String, serde_json::Value, i64)> = JoinSet::new();
        let text = message.content.text.clone();

        {
            let t = text.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let lang = mp1_language(&t);
                (
                    "phase0:language".to_string(),
                    serde_json::json!(lang),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let t = text.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let (intent, sentiment, tone) = mp2_classify(&t);
                (
                    "phase0:classification".to_string(),
                    serde_json::json!({
                        "intent": intent,
                        "sentiment": sentiment,
                        "tone": tone
                    }),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let t = text.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let (topics, keywords) = mp3_topics_keywords(&t);
                (
                    "phase0:topics_keywords".to_string(),
                    serde_json::json!({
                        "topics": topics,
                        "keywords": keywords
                    }),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let t = text.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let entities = mp4_entities_simple(&t);
                (
                    "phase0:entities".to_string(),
                    serde_json::json!(entities),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let t = text.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let det = analyze_all(&t, 0);
                (
                    "phase0:detectors".to_string(),
                    serde_json::to_value(&det).unwrap_or(serde_json::Value::Null),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let rt = self.runtime.clone();
            let msg = message.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let assessment = mp5_complexity(rt.clone(), &msg).await;
                (
                    "phase0:complexity".to_string(),
                    serde_json::to_value(&assessment).unwrap_or(serde_json::Value::Null),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let rt_arc = self.runtime.clone();
            let enabled = {
                let rt = rt_arc.read().unwrap();
                rt.get_setting("ui:phase0_embeddings")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false)
            };
            let adapter_opt = rt_arc.read().unwrap().adapter.read().unwrap().clone();
            let handler_opt = rt_arc
                .read()
                .unwrap()
                .models
                .read()
                .unwrap()
                .get("TEXT_EMBEDDING")
                .and_then(|v| v.first().map(|p| p.handler.clone()));
            let prompt_text = {
                let rt = rt_arc.read().unwrap();
                rt.get_setting("ui:lastPrompt")
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| text.clone())
            };
            let msg_owned = message.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let done = mp6_embedding_queue(rt_arc.clone(), &msg_owned, prompt_text).await;
                (
                    "phase0:embedding_queue".to_string(),
                    serde_json::json!(done),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let rt = self.runtime.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let actions = mp7_available_actions(rt.clone());
                (
                    "phase0:available_actions".to_string(),
                    serde_json::json!(actions),
                    st.elapsed().as_millis() as i64,
                )
            });
        }
        {
            let rt = self.runtime.clone();
            js.spawn(async move {
                let st = std::time::Instant::now();
                let candidates: Vec<String> = mp8_agent_candidates(rt.clone());
                (
                    "phase0:agent_candidates".to_string(),
                    serde_json::json!(candidates),
                    st.elapsed().as_millis() as i64,
                )
            });
        }

        let mut out = Phase0Output::new();
        let start = std::time::Instant::now();
        let mut profiles: Vec<(String, i64)> = Vec::new();
        while let Some(res) = js.join_next().await {
            if let Ok((key, val, dur)) = res {
                let mut rt = self.runtime.write().unwrap();
                rt.set_setting(&key, val.clone(), false);
                profiles.push((key.clone(), dur));
                match key.as_str() {
                    "phase0:language" => {
                        out.language = val.as_str().map(|s| s.to_string());
                    }
                    "phase0:classification" => {
                        if let Some(obj) = val.as_object() {
                            out.intent = obj
                                .get("intent")
                                .and_then(|v| v.as_str().map(|s| s.to_string()));
                            out.sentiment = obj
                                .get("sentiment")
                                .and_then(|v| v.as_str().map(|s| s.to_string()));
                            out.tone = obj
                                .get("tone")
                                .and_then(|v| v.as_str().map(|s| s.to_string()));
                        }
                    }
                    "phase0:topics_keywords" => {
                        if let Some(obj) = val.as_object() {
                            if let Some(arr) = obj.get("topics").and_then(|v| v.as_array()) {
                                out.topics = arr
                                    .iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect();
                            }
                            if let Some(arr) = obj.get("keywords").and_then(|v| v.as_array()) {
                                out.keywords = arr
                                    .iter()
                                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                    .collect();
                                let (metaphones, sim_hint) =
                                    mp9_keywords_phonetic_similarity(&out.keywords);
                                rt.set_setting(
                                    "phase0:keywords:phonetic",
                                    serde_json::json!(metaphones),
                                    false,
                                );
                                rt.set_setting(
                                    "phase0:keywords:similarity_hint",
                                    serde_json::json!(sim_hint),
                                    false,
                                );
                                // If embeddings are queued, schedule background task
                                let queued = rt
                                    .get_setting("phase0:embedding:queued")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                // Background embedding will be processed by runtime services
                            }
                        }
                    }
                    "phase0:entities" => {
                        if let Some(arr) = val.as_array() {
                            out.entities = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    "phase0:detectors" => {
                        if let Some(obj) = val.as_object() {
                            if let Some(amb) = obj.get("ambiguity_score").and_then(|v| v.as_f64()) {
                                let mut rt = self.runtime.write().unwrap();
                                rt.set_setting("ui:ambiguity", serde_json::json!(amb), false);
                            }
                            if let Some(hint) =
                                obj.get("urgency_markers").and_then(|v| v.as_array())
                            {
                                let mut rt = self.runtime.write().unwrap();
                                rt.set_setting(
                                    "ui:urgency_markers",
                                    serde_json::json!(hint.len()),
                                    false,
                                );
                            }
                            if let Some(incomplete) =
                                obj.get("incomplete").and_then(|v| v.as_bool())
                            {
                                let mut rt = self.runtime.write().unwrap();
                                rt.set_setting(
                                    "ui:incomplete",
                                    serde_json::json!(incomplete),
                                    false,
                                );
                            }
                        }
                    }
                    "phase0:complexity" => {
                        if !val.is_null() {
                            if let Ok(assess) =
                                serde_json::from_value::<ComplexityAssessment>(val.clone())
                            {
                                out.complexity = Some(assess);
                            }
                        }
                    }
                    "phase0:available_actions" => {
                        if let Some(arr) = val.as_array() {
                            out.available_actions = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    "phase0:embedding_queue" => {}
                    "phase0:agent_candidates" => {
                        if let Some(arr) = val.as_array() {
                            out.agent_candidates = arr
                                .iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect();
                        }
                    }
                    _ => {}
                }
            }
        }
        let elapsed = start.elapsed().as_millis() as i64;
        profiles.sort_by(|a, b| b.1.cmp(&a.1));
        let top: Vec<String> = profiles
            .iter()
            .take(3)
            .map(|(k, d)| format!("{}:{}ms", k, d))
            .collect();
        debug!(total_ms = elapsed, top = ?top, "phase0_profiles");
        {
            let mut rt = self.runtime.write().unwrap();
            rt.set_setting(
                "phase0:profile",
                serde_json::json!({"total_ms": elapsed, "top": top}),
                false,
            );
        }
        info!(duration_ms = elapsed, "phase0_complete");
        Ok(out)
    }
}

fn detect_language(text: &str) -> String {
    let s = text.to_lowercase();
    if s.contains("¿") || s.contains("¡") || s.contains(" hola ") || s.contains(" gracias ") {
        return "es".to_string();
    }
    if s.contains(" bonjour ") || s.contains(" merci ") || s.contains(" français ") {
        return "fr".to_string();
    }
    if s.contains(" hallo ") || s.contains(" danke ") {
        return "de".to_string();
    }
    "en".to_string()
}

fn classify_text(text: &str) -> (String, String, String) {
    let t = text.to_lowercase();
    let intent =
        if t.contains('?') || t.starts_with("what") || t.starts_with("how") || t.starts_with("why")
        {
            "Question"
        } else if t.contains("please") || t.starts_with("can you") {
            "Request"
        } else if t.contains("hello") || t.contains("hi") {
            "Greeting"
        } else {
            "Statement"
        };
    let positive = ["great", "thanks", "awesome", "good", "love"];
    let negative = ["bad", "hate", "terrible", "awful", "worse"];
    let mut pos = 0;
    let mut neg = 0;
    for w in positive.iter() {
        if t.contains(w) {
            pos += 1;
        }
    }
    for w in negative.iter() {
        if t.contains(w) {
            neg += 1;
        }
    }
    let sentiment = if pos > neg {
        "Positive"
    } else if neg > pos {
        "Negative"
    } else {
        "Neutral"
    };
    let tone = if t.contains("please") || t.contains("thank") {
        "Formal"
    } else if t.contains("lol") || t.contains(":)") {
        "Casual"
    } else {
        "Professional"
    };
    (intent.to_string(), sentiment.to_string(), tone.to_string())
}

fn extract_topics_keywords(text: &str) -> (Vec<String>, Vec<String>) {
    let stop = [
        "the", "a", "an", "and", "or", "to", "of", "in", "on", "for", "with", "is", "it", "this",
        "that", "i", "you", "we", "they", "be", "are", "was", "were", "as", "at",
    ];
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for token in text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
    {
        let lc = token.to_lowercase();
        if stop.contains(&lc.as_str()) {
            continue;
        }
        *freq.entry(lc).or_insert(0) += 1;
    }
    let mut items: Vec<(String, usize)> = freq.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));
    let keywords: Vec<String> = items.iter().take(8).map(|(k, _)| k.clone()).collect();
    let topics = keywords.iter().take(4).cloned().collect();
    (topics, keywords)
}

fn extract_entities_simple(text: &str) -> Vec<String> {
    let mut entities = Vec::new();
    let mut current = Vec::new();
    for w in text.split_whitespace() {
        let first = w.chars().next();
        if let Some(c) = first {
            if c.is_uppercase() {
                current.push(w.trim_matches(|ch: char| !ch.is_alphanumeric()).to_string());
            } else {
                if current.len() >= 1 {
                    entities.push(current.join(" "));
                    current.clear();
                }
            }
        } else {
            if current.len() >= 1 {
                entities.push(current.join(" "));
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        entities.push(current.join(" "));
    }
    let mut uniq = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in entities {
        if !e.is_empty() && uniq.insert(e.to_lowercase()) {
            out.push(e);
        }
    }
    out
}
