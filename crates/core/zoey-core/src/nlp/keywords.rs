pub fn extract_keywords(text: &str, max: usize) -> Vec<String> {
    let stop = ["the","a","an","and","or","to","of","in","on","for","with","is","it","this","that","i","you","we","they","be","are","was","were","as","at","from","by","about","into","over","after","before"];
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for token in text.split(|c: char| !c.is_alphanumeric()).filter(|s| !s.is_empty()) {
        let lc = token.to_lowercase();
        if stop.contains(&lc.as_str()) { continue; }
        *freq.entry(lc).or_insert(0) += 1;
    }
    let mut items: Vec<(String, usize)> = freq.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1));
    items.into_iter().take(max).map(|(k, _)| k).collect()
}
