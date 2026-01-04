use crate::detectors::analyze_all;
use crate::testing::create_test_memory;

#[test]
fn test_ambiguity_and_incomplete() {
    let text = "It is not good...";
    let det = analyze_all(text, 0);
    assert!(det.vague_pronouns >= 1);
    assert!(det.incomplete);
}

#[test]
fn test_question_extraction() {
    let text = "What is Rust? How do I build?";
    let det = analyze_all(text, 0);
    assert!(det.questions.len() >= 2);
}

