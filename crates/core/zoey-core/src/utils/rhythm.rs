use crate::types::*;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct ConversationRhythm {
    pub avg_user_message_length: f32,
    pub message_velocity: f32,
    pub recent_topics: Vec<String>,
    pub suggested_response_length: String,
}

impl ConversationRhythm {
    pub fn new() -> Self {
        Self {
            avg_user_message_length: 0.0,
            message_velocity: 0.0,
            recent_topics: Vec::new(),
            suggested_response_length: "moderate".to_string(),
        }
    }
}

pub struct RhythmTracker {
    window: VecDeque<(i64, usize)>,
    max_window: usize,
}

impl RhythmTracker {
    pub fn new() -> Self {
        Self {
            window: VecDeque::new(),
            max_window: 10,
        }
    }
    pub fn update(&mut self, msg: &Memory, topics: &[String]) -> ConversationRhythm {
        let now = chrono::Utc::now().timestamp();
        self.window.push_back((now, msg.content.text.len()));
        while self.window.len() > self.max_window {
            self.window.pop_front();
        }
        let avg_len = if self.window.is_empty() {
            0.0
        } else {
            self.window.iter().map(|(_, l)| *l as f32).sum::<f32>() / (self.window.len() as f32)
        };
        let velocity = if self.window.len() >= 2 {
            let first = self.window.front().unwrap().0;
            let last = self.window.back().unwrap().0;
            let dt = (last - first) as f32;
            if dt <= 0.0 {
                0.0
            } else {
                (self.window.len() as f32) / (dt / 60.0)
            } // msgs per minute
        } else {
            0.0
        };
        let suggested = if velocity > 5.0 {
            "terse"
        } else if velocity > 2.0 {
            "brief"
        } else if avg_len > 300.0 {
            "detailed"
        } else {
            "moderate"
        };
        ConversationRhythm {
            avg_user_message_length: avg_len,
            message_velocity: velocity,
            recent_topics: topics.to_vec(),
            suggested_response_length: suggested.to_string(),
        }
    }
}
