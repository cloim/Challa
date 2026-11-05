//! Windows two-set Korean rendering for keyword aliases and erase accounting.
const INITIAL: &str = "ㄱㄲㄴㄷㄸㄹㅁㅂㅃㅅㅆㅇㅈㅉㅊㅋㅌㅍㅎ";
const VOWEL: &str = "ㅏㅐㅑㅒㅓㅔㅕㅖㅗㅘㅙㅚㅛㅜㅝㅞㅟㅠㅡㅢㅣ";
const FINAL: &str = " ㄱㄲㄳㄴㄵㄶㄷㄹㄺㄻㄼㄽㄾㄿㅀㅁㅂㅄㅅㅆㅇㅈㅊㅋㅌㅍㅎ";

fn index(table: &str, ch: char) -> Option<usize> {
    table.chars().position(|c| c == ch)
}
fn syllable(l: usize, v: usize, t: usize) -> char {
    char::from_u32(0xac00 + ((l * 21 + v) * 28 + t) as u32).unwrap()
}
fn parts(ch: char) -> Option<(usize, usize, usize)> {
    let n = ch as u32;
    (0xac00..=0xd7a3).contains(&n).then(|| {
        let n = (n - 0xac00) as usize;
        (n / 588, n % 588 / 28, n % 28)
    })
}
fn combine_vowel(a: usize, b: usize) -> Option<usize> {
    match (a, b) {
        (8, 0) => Some(9),
        (8, 1) => Some(10),
        (8, 20) => Some(11),
        (13, 4) => Some(14),
        (13, 5) => Some(15),
        (13, 20) => Some(16),
        (18, 20) => Some(19),
        (9, 20) => Some(10),
        (14, 20) => Some(15),
        _ => None,
    }
}
fn combine_final(a: usize, b: usize) -> Option<usize> {
    match (a, b) {
        (1, 19) => Some(3),
        (4, 22) => Some(5),
        (4, 27) => Some(6),
        (8, 1) => Some(9),
        (8, 16) => Some(10),
        (8, 17) => Some(11),
        (8, 19) => Some(12),
        (8, 25) => Some(13),
        (8, 26) => Some(14),
        (8, 27) => Some(15),
        (17, 19) => Some(18),
        _ => None,
    }
}
fn split_final(t: usize) -> (usize, usize) {
    match t {
        3 => (1, 19),
        5 => (4, 22),
        6 => (4, 27),
        9 => (8, 1),
        10 => (8, 16),
        11 => (8, 17),
        12 => (8, 19),
        13 => (8, 25),
        14 => (8, 26),
        15 => (8, 27),
        18 => (17, 19),
        _ => (0, t),
    }
}
fn jamo(ch: char) -> char {
    match ch {
        'R' => 'ㄲ',
        'E' => 'ㄸ',
        'Q' => 'ㅃ',
        'T' => 'ㅆ',
        'W' => 'ㅉ',
        'O' => 'ㅒ',
        'P' => 'ㅖ',
        _ => match ch.to_ascii_lowercase() {
            'r' => 'ㄱ',
            's' => 'ㄴ',
            'e' => 'ㄷ',
            'f' => 'ㄹ',
            'a' => 'ㅁ',
            'q' => 'ㅂ',
            't' => 'ㅅ',
            'd' => 'ㅇ',
            'w' => 'ㅈ',
            'c' => 'ㅊ',
            'z' => 'ㅋ',
            'x' => 'ㅌ',
            'v' => 'ㅍ',
            'g' => 'ㅎ',
            'k' => 'ㅏ',
            'o' => 'ㅐ',
            'i' => 'ㅑ',
            'j' => 'ㅓ',
            'p' => 'ㅔ',
            'u' => 'ㅕ',
            'h' => 'ㅗ',
            'y' => 'ㅛ',
            'n' => 'ㅜ',
            'b' => 'ㅠ',
            'm' => 'ㅡ',
            'l' => 'ㅣ',
            _ => ch,
        },
    }
}

pub fn render(keys: &str) -> String {
    let mut result = Vec::new();
    for ch in keys.chars().map(jamo) {
        if let Some(v) = index(VOWEL, ch) {
            if let Some(&last) = result.last() {
                if let Some(l) = index(INITIAL, last) {
                    *result.last_mut().unwrap() = syllable(l, v, 0);
                    continue;
                }
                if let Some(a) = index(VOWEL, last).and_then(|a| combine_vowel(a, v)) {
                    *result.last_mut().unwrap() = VOWEL.chars().nth(a).unwrap();
                    continue;
                }
                if let Some((l, old_v, t)) = parts(last) {
                    if t == 0 {
                        if let Some(v) = combine_vowel(old_v, v) {
                            *result.last_mut().unwrap() = syllable(l, v, 0);
                            continue;
                        }
                    } else {
                        let (left, right) = split_final(t);
                        let consonant = FINAL.chars().nth(right).unwrap();
                        if let Some(next_l) = index(INITIAL, consonant) {
                            *result.last_mut().unwrap() = syllable(l, old_v, left);
                            result.push(syllable(next_l, v, 0));
                            continue;
                        }
                    }
                }
            }
        } else if let Some(t) = index(FINAL, ch).filter(|t| *t != 0) {
            if let Some((l, v, old_t)) = result.last().copied().and_then(parts) {
                if old_t == 0 {
                    *result.last_mut().unwrap() = syllable(l, v, t);
                    continue;
                }
                if let Some(t) = combine_final(old_t, t) {
                    *result.last_mut().unwrap() = syllable(l, v, t);
                    continue;
                }
            }
        }
        result.push(ch);
    }
    result.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn renders_two_set_syllables_and_the_withheld_key_prefix() {
        for (keys, text) in [
            ("#rksk", "#가나"),
            ("#rks", "#간"),
            ("dkssudgktpdy", "안녕하세요"),
            ("rkqt", "값"),
            ("rkqtk", "갑사"),
            ("rhk", "과"),
            ("Rk", "까"),
            ("a b", "ㅁ ㅠ"),
        ] {
            assert_eq!(render(keys), text, "{keys}");
        }
    }
}
