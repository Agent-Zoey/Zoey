use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct QuestionInfo {
    pub text: String,
    pub score: f32,
    pub entities: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DerivedDetections {
    pub ambiguity_score: f32,
    pub vague_pronouns: usize,
    pub weak_verbs: usize,
    pub has_svo: bool,
    pub short_message: bool,
    pub casual_markers: Vec<String>,
    pub formal_markers: Vec<String>,
    pub polite_markers: Vec<String>,
    pub rude_markers: Vec<String>,
    pub sentiment_positive: usize,
    pub sentiment_negative: usize,
    pub excitement_markers: usize,
    pub frustration_markers: usize,
    pub hesitation_markers: Vec<String>,
    pub urgency_markers: Vec<String>,
    pub questions: Vec<QuestionInfo>,
    pub negators: Vec<String>,
    pub unresolved_references: Vec<String>,
    pub comparisons: Vec<String>,
    pub conditionals: Vec<String>,
    pub enumerations: Vec<String>,
    pub incomplete: bool,
    pub incomplete_reason: Option<String>,
}

pub fn analyze_all(text: &str, recent_context_len: usize) -> DerivedDetections {
    let t = text.to_lowercase();
    let tokens: Vec<&str> = t
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();

    let mut det = DerivedDetections::default();

    let vague = ["it", "this", "that"];
    let weak = ["do", "make", "get"];
    let subjects = ["i", "we", "you", "they", "he", "she", "it"]; // allow pronoun subjects
    let verbs = [
        "is", "are", "was", "were", "do", "does", "did", "make", "get", "have", "has", "had",
    ];

    let mut vp = 0usize;
    let mut wv = 0usize;
    for tok in &tokens {
        if vague.contains(tok) {
            vp += 1;
        }
        if weak.contains(tok) {
            wv += 1;
        }
    }
    det.vague_pronouns = vp;
    det.weak_verbs = wv;

    let has_subject = tokens.iter().any(|x| subjects.contains(x));
    let has_verb = tokens.iter().any(|x| verbs.contains(x));
    let has_object = tokens.len() > 4; // naive proxy
    det.has_svo = has_subject && has_verb && has_object;
    det.short_message = tokens.len() < 4;

    // Ambiguity score: base from vague + weak + structure penalty
    let mut score = (vp as f32) * 0.6 + (wv as f32) * 0.4 + if det.has_svo { 0.0 } else { 1.5 };
    if det.short_message {
        score += 0.8;
    }
    if recent_context_len > 0 {
        score *= 0.8; // dampen if conversation context exists
    }
    det.ambiguity_score = score;

    // Formality & Politeness
    let casual = ["yo", "lol", "gonna", "u", "ur"];
    let formal = ["would", "kindly", "regarding", "accordingly", "appreciate"];
    let polite = ["please", "thank you", "thanks", "appreciated"];
    let profanity = ["damn", "shit", "fuck", "bitch", "asshole"];
    for m in casual.iter() {
        if t.contains(m) {
            det.casual_markers.push(m.to_string());
        }
    }
    for m in formal.iter() {
        if t.contains(m) {
            det.formal_markers.push(m.to_string());
        }
    }
    for m in polite.iter() {
        if t.contains(m) {
            det.polite_markers.push(m.to_string());
        }
    }
    for m in profanity.iter() {
        if t.contains(m) {
            det.rude_markers.push(m.to_string());
        }
    }

    // Sentiment
    let positive = ["great", "awesome", "good", "love", "excellent", "nice"];
    let negative = ["bad", "hate", "terrible", "awful", "worse", "broken"];
    det.sentiment_positive = positive.iter().filter(|w| t.contains(*w)).count();
    det.sentiment_negative = negative.iter().filter(|w| t.contains(*w)).count();
    det.excitement_markers = t.matches('!').count()
        + if text.chars().any(|c| c.is_uppercase()) {
            1
        } else {
            0
        };
    det.frustration_markers = det.rude_markers.len()
        + if t.contains("again") || t.contains("still broken") {
            1
        } else {
            0
        };

    // Hesitation & Urgency
    let hes = ["maybe", "i think", "not sure", "possibly", "perhaps"];
    let urg = ["asap", "urgent", "now", "immediately", "deadline"];
    for m in hes.iter() {
        if t.contains(m) {
            det.hesitation_markers.push(m.to_string());
        }
    }
    for m in urg.iter() {
        if t.contains(m) {
            det.urgency_markers.push(m.to_string());
        }
    }

    // Question Extraction
    let mut qs: Vec<String> = Vec::new();
    for part in text.split('?') {
        let s = part.trim();
        if s.is_empty() {
            continue;
        }
        if s.starts_with("what")
            || s.starts_with("how")
            || s.starts_with("why")
            || s.starts_with("where")
            || s.starts_with("when")
            || s.starts_with("which")
        {
            qs.push(s.to_string());
        }
    }
    if text.contains('?') {
        for seg in text.split('?') {
            let s = seg.trim();
            if !s.is_empty() {
                qs.push(s.to_string());
            }
        }
    }
    let mut seen = HashSet::new();
    let mut qi: Vec<QuestionInfo> = Vec::new();
    for q in qs {
        let key = q.to_lowercase();
        if !seen.insert(key.clone()) {
            continue;
        }
        let score = if q.len() < 10 { 0.5 } else { 0.8 };
        let ent = extract_entities_simple(&q);
        qi.push(QuestionInfo {
            text: q,
            score,
            entities: ent,
        });
    }
    det.questions = qi;

    // Negation & References
    let negators = ["not", "don't", "never", "except", "no", "without"];
    let refs = ["it", "that", "the last one", "this"];
    det.negators = negators
        .iter()
        .filter(|w| t.contains(*w))
        .map(|s| s.to_string())
        .collect();
    det.unresolved_references = refs
        .iter()
        .filter(|w| t.contains(*w))
        .map(|s| s.to_string())
        .collect();

    // Comparisons & Conditionals & Enumerations
    let comp_k = [
        " vs ",
        "better than",
        "difference between",
        "compare",
        "comparison",
    ];
    let cond_k = [" if ", " when ", " unless ", " provided that "];
    let enum_signals = [",", ";", "1.", "2.", "- "];
    det.comparisons = comp_k
        .iter()
        .filter(|w| t.contains(*w))
        .map(|s| s.trim().to_string())
        .collect();
    det.conditionals = cond_k
        .iter()
        .filter(|w| t.contains(*w))
        .map(|s| s.trim().to_string())
        .collect();
    det.enumerations = enum_signals
        .iter()
        .filter(|w| t.contains(*w))
        .map(|s| s.trim().to_string())
        .collect();

    // Completeness
    let trailing_ellipsis = text.trim_end().ends_with("...");
    let trailing_dash = text.trim_end().ends_with("--") || text.trim_end().ends_with("-");
    let ends_with_conj = t.trim_end().ends_with(" and")
        || t.trim_end().ends_with(" or")
        || t.trim_end().ends_with(" but");
    let fragment = tokens.len() < 2 || (!det.has_svo && tokens.len() < 5);
    det.incomplete = trailing_ellipsis || trailing_dash || ends_with_conj || fragment;
    det.incomplete_reason = if trailing_ellipsis {
        Some("ellipsis".to_string())
    } else if trailing_dash {
        Some("dash".to_string())
    } else if ends_with_conj {
        Some("conjunction".to_string())
    } else if fragment {
        Some("fragment".to_string())
    } else {
        None
    };

    det
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
