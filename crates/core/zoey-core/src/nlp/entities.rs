pub fn extract_entities_regex(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"\b([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,3})\b").unwrap();
    let mut out = Vec::new();
    let mut uniq = std::collections::HashSet::new();
    for cap in re.captures_iter(text) {
        let e = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        if !e.is_empty() && uniq.insert(e.to_lowercase()) { out.push(e); }
    }
    out
}
