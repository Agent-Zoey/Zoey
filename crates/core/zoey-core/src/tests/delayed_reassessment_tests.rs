use crate::{create_mock_runtime, create_test_memory, MessageProcessor, testing::create_test_room};
use std::sync::{Arc, RwLock};

#[tokio::test]
async fn test_delayed_reassessment_window() {
    let runtime = create_mock_runtime();
    {
        let mut rt = runtime.write().unwrap();
        rt.set_setting("AUTONOMOUS_DELAYED_REASSESSMENT", serde_json::json!(true), false);
        rt.set_setting("ui:phase0_enabled", serde_json::json!(false), false);
        rt.set_setting("ui:incomplete", serde_json::json!(true), false);
    }
    let processor = MessageProcessor::new(runtime.clone());
    let room = create_test_room(crate::types::ChannelType::Dm);
    let m1 = create_test_memory("I need help with Rust...");
    let r1 = processor.process_message(m1.clone(), room.clone()).await.expect("ok");
    assert!(r1.is_empty());
    let m2 = create_test_memory("Specifically lifetimes.");
    let r2 = processor.process_message(m2.clone(), room.clone()).await.expect("ok");
    // After merge, phase0 disabled so response still may be empty, but window cleared
    let rt = runtime.read().unwrap();
    let pend = rt.get_setting(&format!("delayed:{}:pending", m1.room_id));
    assert!(pend.is_null() || pend.as_str().unwrap_or("").is_empty());
}

