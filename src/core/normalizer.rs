use std::collections::HashSet;

/// Converts full-width ASCII (U+FF01..U+FF5E) and ideographic space (U+3000) to half-width.
pub fn to_half_width(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch as u32 {
            0x3000 => out.push(' '),
            0xFF01..=0xFF5E => {
                if let Some(hw) = char::from_u32(ch as u32 - 0xFEE0) {
                    out.push(hw);
                }
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Normalizes a raw biomarker name:
/// `trim -> half-width -> uppercase -> remove whitespace and *#()（）[]【】`.
pub fn clean_biomarker_name(raw: &str) -> String {
    let hw = to_half_width(raw.trim()).to_uppercase();
    hw.chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '*' | '#' | '(' | ')' | '（' | '）' | '[' | ']' | '【' | '】' | '《' | '》'
                )
        })
        .collect()
}

/// Strips a leading alphanumeric test-code column (e.g. `ALD2 醛固酮（立位）`)
/// and posture tokens (`立位` / `卧位` / `坐位`) before cleaning, for better
/// dictionary and fuzzy matching on multi-column hospital reports.
pub fn strip_code_and_posture(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_prefix = match trimmed.split_once(char::is_whitespace) {
        Some((first, rest))
            if !first.is_empty()
                && first.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
                && rest.chars().any(|c| !c.is_ascii()) =>
        {
            rest
        }
        _ => trimmed,
    };
    let cleaned = clean_biomarker_name(without_prefix);
    cleaned
        .replace("立位", "")
        .replace("卧位", "")
        .replace("坐位", "")
}

/// Normalizes an OCR-extracted unit and fixes common OCR typos.
pub fn normalize_unit(raw_unit: Option<&str>) -> Option<String> {
    let raw = raw_unit?;
    let hw = to_half_width(raw.trim());
    if hw.is_empty() {
        return None;
    }
    let compact: String = hw.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.is_empty() {
        return None;
    }
    let mapped = match compact.to_lowercase().as_str() {
        "mlu/l" | "miu/l" => "mIU/L",
        "mlu/ml" => "mIU/mL",
        "uiu/ml" | "μiu/ml" => "uIU/mL",
        "umol/l" | "μmol/l" => "umol/L",
        "mmol/l" => "mmol/L",
        "pmol/l" => "pmol/L",
        "ng/dl" => "ng/dL",
        "pg/ml" | "dg/ml" | "p8/ml" | "pg/mi" => "pg/mL",
        "ug/mi" => "ug/mL",
        "ng/ml" => "ng/mL",
        "ng/ml/h" => "ng/mL/h",
        "ug/l/h" | "μg/l/h" => "ug/L/h",
        "pmol/l/min" => "pmol/L/min",
        "mg/dl" => "mg/dL",
        "meq/l" => "mEq/L",
        "iu/l" => "IU/L",
        "ml/min/1.73m2" => "mL/min/1.73m2",
        "ml/min" => "mL/min",
        _ => return Some(compact),
    };
    Some(mapped.to_owned())
}

pub fn jaro(s1: &str, s2: &str) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    let c1: Vec<char> = s1.chars().collect();
    let c2: Vec<char> = s2.chars().collect();
    if c1.is_empty() || c2.is_empty() {
        return 0.0;
    }

    let len1 = c1.len();
    let len2 = c2.len();
    let match_dist = (len1.max(len2) / 2).saturating_sub(1);

    let mut s1_matches = vec![false; len1];
    let mut s2_matches = vec![false; len2];
    let mut matches = 0usize;

    for i in 0..len1 {
        let start = i.saturating_sub(match_dist);
        let end = (i + match_dist + 1).min(len2);
        for j in start..end {
            if s2_matches[j] || c1[i] != c2[j] {
                continue;
            }
            s1_matches[i] = true;
            s2_matches[j] = true;
            matches += 1;
            break;
        }
    }

    if matches == 0 {
        return 0.0;
    }

    let mut transpositions = 0usize;
    let mut k = 0usize;
    for i in 0..len1 {
        if !s1_matches[i] {
            continue;
        }
        while !s2_matches[k] {
            k += 1;
        }
        if c1[i] != c2[k] {
            transpositions += 1;
        }
        k += 1;
    }

    let m = matches as f64;
    ((m / len1 as f64) + (m / len2 as f64) + ((m - transpositions as f64 / 2.0) / m)) / 3.0
}

pub fn jaro_winkler(s1: &str, s2: &str) -> f64 {
    let j = jaro(s1, s2);
    if (j - 1.0).abs() < f64::EPSILON {
        return 1.0;
    }
    let prefix = s1
        .chars()
        .zip(s2.chars())
        .take(4)
        .take_while(|(a, b)| a == b)
        .count();
    j + (prefix as f64) * 0.1 * (1.0 - j)
}

pub fn jaccard_2gram(s1: &str, s2: &str) -> f64 {
    if s1 == s2 {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    let g1 = to_2grams(s1);
    let g2 = to_2grams(s2);
    let intersection = g1.intersection(&g2).count();
    let union = g1.union(&g2).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

fn to_2grams(s: &str) -> HashSet<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return HashSet::from([s.to_owned()]);
    }
    chars
        .windows(2)
        .map(|w| w.iter().collect::<String>())
        .collect()
}

/// Combined text similarity per SPEC Section 5 Stage 2:
/// `0.6 * JaroWinkler + 0.4 * Jaccard2Gram`.
pub fn combined_text_score(s1: &str, s2: &str) -> f64 {
    0.6 * jaro_winkler(s1, s2) + 0.4 * jaccard_2gram(s1, s2)
}
