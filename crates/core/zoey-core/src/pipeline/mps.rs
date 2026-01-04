use crate::types::*;
use crate::Result;

pub fn mp1_language(text: &str) -> String {
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

pub fn mp2_classify(text: &str) -> (String, String, String) {
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

pub fn mp3_topics_keywords(text: &str) -> (Vec<String>, Vec<String>) {
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

pub fn mp4_entities_simple(text: &str) -> Vec<String> {
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

pub async fn mp5_complexity(
    runtime: std::sync::Arc<std::sync::RwLock<crate::AgentRuntime>>,
    message: &Memory,
) -> Option<crate::planner::complexity::ComplexityAssessment> {
    let analyzer = crate::planner::complexity::ComplexityAnalyzer::new();
    let state = State::new();
    analyzer.assess(message, &state).await.ok()
}

pub async fn mp6_embedding_queue(
    runtime: std::sync::Arc<std::sync::RwLock<crate::AgentRuntime>>,
    message: &Memory,
    text: String,
) -> bool {
    let rt_arc = runtime;
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
    let mut updated = false;
    if enabled {
        if let (Some(adapter), Some(handler)) = (adapter_opt, handler_opt) {
            let params = crate::types::GenerateTextParams {
                prompt: prompt_text,
                max_tokens: None,
                temperature: None,
                top_p: None,
                stop: None,
                model: None,
                frequency_penalty: None,
                presence_penalty: None,
            };
            let mh_params = crate::types::ModelHandlerParams {
                runtime: std::sync::Arc::new(()),
                params,
            };
            if let Ok(json) = (handler)(mh_params).await {
                if let Ok(vec) = serde_json::from_str::<Vec<f32>>(&json) {
                    let mut mem = message.clone();
                    mem.embedding = Some(vec);
                    let _ = adapter.update_memory(&mem).await;
                    updated = true;
                }
            }
        }
    }
    if let Ok(mut rt) = rt_arc.write() {
        let key = if enabled && updated {
            "phase0:embedding:updated"
        } else {
            "phase0:embedding:queued"
        };
        rt.set_setting(key, serde_json::json!(true), false);
    }
    enabled && updated
}

pub fn mp7_available_actions(
    runtime: std::sync::Arc<std::sync::RwLock<crate::AgentRuntime>>,
) -> Vec<String> {
    let r = runtime.read().unwrap();
    let names: Vec<String> = {
        let ag = r.actions.read().unwrap();
        ag.iter().map(|a| a.name().to_string()).collect()
    };
    names
}

pub fn mp8_agent_candidates(
    runtime: std::sync::Arc<std::sync::RwLock<crate::AgentRuntime>>,
) -> Vec<String> {
    let r = runtime.read().unwrap();
    let mut out: Vec<String> = Vec::new();
    if let Some(svc) = r.get_service_by_name("multi-agent-coordination") {
        let capability = if let Some(intent) = r
            .get_setting("ui:intent")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
        {
            match intent.as_str() {
                "Question" => "qa",
                "Request" => "task_execution",
                _ => "conversation",
            }
        } else {
            "conversation"
        };
        if let Some(cands) = svc.query_agents(capability) {
            for (id, _score) in cands {
                out.push(id.to_string());
            }
        }
    }
    out
}

pub fn mp9_keywords_phonetic_similarity(keywords: &[String]) -> (Vec<(String, String)>, f32) {
    use crate::nlp::{double_metaphone, normalized_similarity};
    let metaphones: Vec<(String, String)> = keywords.iter().map(|k| double_metaphone(k)).collect();
    let sim_hint: f32 = if keywords.len() > 1 {
        normalized_similarity(&keywords[0], &keywords[1])
    } else {
        0.0
    };
    (metaphones, sim_hint)
}
