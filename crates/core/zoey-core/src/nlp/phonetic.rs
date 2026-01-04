pub fn double_metaphone(s: &str) -> (String, String) {
    let k = soundex_key(s);
    (k.clone(), k)
}

pub fn soundex_key(s: &str) -> String {
    let mut out = String::new();
    let mut last = '0';
    for c in s.chars() {
        if !c.is_ascii_alphabetic() { continue; }
        let cl = c.to_ascii_uppercase();
        if out.is_empty() { out.push(cl); continue; }
        let code = match cl {
            'B'|'F'|'P'|'V' => '1',
            'C'|'G'|'J'|'K'|'Q'|'S'|'X'|'Z' => '2',
            'D'|'T' => '3',
            'L' => '4',
            'M'|'N' => '5',
            'R' => '6',
            _ => '0',
        };
        if code != '0' && code != last { out.push(code); last = code; }
        if out.len() >= 4 { break; }
    }
    while out.len() < 4 { out.push('0'); }
    out
}
