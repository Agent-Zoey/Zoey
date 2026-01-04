use crate::preprocessor::Phase0Preprocessor;
use crate::{create_mock_runtime, create_test_memory};
use crate::utils::ConversationRhythm;

#[tokio::test]
async fn test_phase0_runs_and_populates_settings() {
    let runtime = create_mock_runtime();
    let msg = create_test_memory("Hello there, can you explain the database optimization approach in Rust?");
    let pre = Phase0Preprocessor::new(runtime.clone());
    let out = pre.execute(&msg).await.expect("phase0 execute");
    assert!(out.intent.is_some());
    assert!(out.tone.is_some());
    assert!(out.keywords.len() >= 1);

    let rt = runtime.read().unwrap();
    let intent = rt.get_setting("phase0:classification");
    assert!(intent.is_some());
    let amb = rt.get_setting("ui:ambiguity");
    assert!(amb.is_some());
}
