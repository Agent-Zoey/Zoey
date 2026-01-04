use std::cmp::min;

pub fn double_metaphone(s: &str) -> (String, String) {
    let mut primary = String::new();
    let mut secondary = String::new();
    let mut prev = '\0';
    for ch in s.chars() {
        let c = ch.to_ascii_lowercase();
        if !c.is_ascii_alphabetic() {
            continue;
        }
        if c == prev {
            continue;
        }
        prev = c;
        if c == 'x' {
            primary.push('K');
            secondary.push('K');
            primary.push('S');
            secondary.push('S');
            continue;
        }
        let p = match c {
            'a' | 'e' | 'i' | 'o' | 'u' => 'A',
            'b' => 'B',
            'c' => 'K',
            'd' => 'T',
            'f' => 'F',
            'g' => 'K',
            'h' => 'H',
            'j' => 'J',
            'k' => 'K',
            'l' => 'L',
            'm' => 'M',
            'n' => 'N',
            'p' => 'P',
            'q' => 'K',
            'r' => 'R',
            's' => 'S',
            't' => 'T',
            'v' => 'F',
            'w' => 'W',
            'y' => 'Y',
            'z' => 'S',
            _ => 'X',
        };
        if p == 'X' {
            continue;
        }
        if p == 'K' {
            primary.push('K');
            secondary.push('K');
            continue;
        }
        if p == 'S' {
            primary.push('S');
            secondary.push('S');
            continue;
        }
        if p == 'A' {
            primary.push('A');
            secondary.push('A');
            continue;
        }
        primary.push(p);
        secondary.push(p);
    }
    (primary, secondary)
}

pub fn normalized_similarity(a: &str, b: &str) -> f32 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: Vec<char> = a.chars().collect();
    let sb: Vec<char> = b.chars().collect();
    let m = sa.len();
    let n = sb.len();
    let mut dp = vec![vec![0usize; n + 1]; m + 1];
    for i in 0..=m {
        dp[i][0] = i;
    }
    for j in 0..=n {
        dp[0][j] = j;
    }
    for i in 1..=m {
        for j in 1..=n {
            let cost = if sa[i - 1] == sb[j - 1] { 0 } else { 1 };
            let del = dp[i - 1][j] + 1;
            let ins = dp[i][j - 1] + 1;
            let sub = dp[i - 1][j - 1] + cost;
            dp[i][j] = min(del, min(ins, sub));
        }
    }
    let dist = dp[m][n] as f32;
    let max_len = std::cmp::max(m, n) as f32;
    1.0 - (dist / max_len)
}
