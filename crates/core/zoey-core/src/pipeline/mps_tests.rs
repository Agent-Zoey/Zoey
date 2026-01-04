#[cfg(test)]
mod tests {
    use super::super::mps::*;
    use crate::{AgentRuntime, RuntimeOpts, types::{Memory, Content}};
    use std::sync::{Arc, RwLock};

    #[test]
    fn test_mp1_language_basic() {
        assert_eq!(mp1_language("hello"), "en");
        assert_eq!(mp1_language("hola"), "es");
        assert_eq!(mp1_language("bonjour"), "fr");
    }

    #[test]
    fn test_mp2_classify() {
        let (intent, sentiment, tone) = mp2_classify("hello, please help?");
        assert_eq!(intent, "Question");
        assert!(matches!(sentiment.as_str(), "Neutral" | "Positive" | "Negative"));
        assert!(matches!(tone.as_str(), "Formal" | "Casual" | "Professional"));
    }

    #[test]
    fn test_mp3_topics_keywords() {
        let (topics, keywords) = mp3_topics_keywords("Rust is great for systems programming and performance");
        assert!(!keywords.is_empty());
        assert!(!topics.is_empty());
    }

    #[test]
    fn test_mp4_entities_simple() {
        let ents = mp4_entities_simple("We met Alice Johnson at Acme Corp in New York City.");
        assert!(ents.iter().any(|e| e.contains("Alice Johnson")));
    }

    #[test]
    fn test_mp9_keywords_phonetic_similarity() {
        let (meta, sim) = mp9_keywords_phonetic_similarity(&["code".to_string(), "coding".to_string()]);
        assert_eq!(meta.len(), 2);
        assert!(sim >= 0.0);
    }

    #[tokio::test]
    async fn test_mp5_complexity_basic() {
        let rt = AgentRuntime::new(RuntimeOpts { test_mode: Some(true), ..Default::default() }).await.unwrap();
        let msg = Memory { id: uuid::Uuid::new_v4(), entity_id: uuid::Uuid::new_v4(), agent_id: uuid::Uuid::new_v4(), room_id: uuid::Uuid::new_v4(), content: Content { text: "Please summarize this paragraph".to_string(), ..Default::default() }, embedding: None, metadata: None, created_at: 0, unique: Some(false), similarity: None };
        let res = mp5_complexity(rt.clone(), &msg).await;
        assert!(res.is_some());
    }

    #[tokio::test]
    async fn test_mp6_embedding_queue_disabled() {
        let rt = AgentRuntime::new(RuntimeOpts { test_mode: Some(true), ..Default::default() }).await.unwrap();
        {
            let mut r = rt.write().unwrap();
            r.set_setting("ui:phase0_embeddings", serde_json::json!(false), false);
        }
        let msg = Memory { id: uuid::Uuid::new_v4(), entity_id: uuid::Uuid::new_v4(), agent_id: uuid::Uuid::new_v4(), room_id: uuid::Uuid::new_v4(), content: Content { text: "Hello".to_string(), ..Default::default() }, embedding: None, metadata: None, created_at: 0, unique: Some(false), similarity: None };
        let done = mp6_embedding_queue(rt.clone(), &msg, "Hello".to_string()).await;
        assert!(!done);
    }
}
