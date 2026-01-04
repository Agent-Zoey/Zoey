pub fn levenshtein(a: &str, b: &str) -> usize {
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len()+1];
    for (i, ca) in a.chars().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b.chars().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j+1] = std::cmp::min(
                std::cmp::min(curr[j] + 1, prev[j+1] + 1),
                prev[j] + cost,
            );
        }
        prev.clone_from_slice(&curr);
    }
    prev[b.len()]
}

pub fn jaro_winkler(a: &str, b: &str) -> f32 {
    // Lightweight approximate implementation (not exact), sufficient for ranking.
    if a.is_empty() && b.is_empty() { return 1.0; }
    let la = a.len() as f32;
    let lb = b.len() as f32;
    let overlap = common_chars(a, b) as f32;
    let base = overlap / la.min(lb);
    (base * 0.9 + 0.1 * prefix_len(a, b) as f32 / 4.0).min(1.0)
}

fn common_chars(a: &str, b: &str) -> usize {
    let mut count = 0;
    let mut seen = std::collections::HashSet::new();
    for c in a.chars() {
        if b.contains(c) && seen.insert(c) { count += 1; }
    }
    count
}

fn prefix_len(a: &str, b: &str) -> usize {
    let mut n = 0;
    for (ca, cb) in a.chars().zip(b.chars()) {
        if ca == cb { n += 1; } else { break; }
        if n >= 4 { break; }
    }
    n
}

pub fn normalized_similarity(a: &str, b: &str) -> f32 {
    let jw = jaro_winkler(a, b);
    let ld = levenshtein(a, b) as f32;
    let len = (a.len().max(b.len()) as f32).max(1.0);
    let lev_sim = 1.0 - (ld / len);
    ((jw + lev_sim) / 2.0).max(0.0).min(1.0)
}
