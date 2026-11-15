use regex::Regex;
use uuid::Uuid;

use super::models::{
    BiomarkerCandidate, DictionaryBundle, OcrBlock, OcrResponse, ParsedLabItem, StandardBiomarker,
};
use super::normalizer::{
    clean_biomarker_name, combined_text_score, normalize_unit, strip_code_and_posture,
};

const ACCEPT_THRESHOLD: f64 = 0.65;

#[derive(Debug, Clone)]
pub struct ReconstructedRow {
    pub raw_name: String,
    pub operator_symbol: String,
    pub raw_value: f64,
    pub raw_unit: Option<String>,
    pub ref_range_raw: Option<String>,
    pub abnormal_flag: String,
    pub row_bbox: [f64; 4],
    pub ocr_confidence: f64,
}

#[derive(Debug, Clone)]
pub struct LayoutReconstructionResult {
    pub rows: Vec<ReconstructedRow>,
    pub detected_posture: String,
    pub detected_date: Option<String>,
}

/// Stage 1: Geometric layout clustering & intra-row column anchor splitting.
pub fn reconstruct_layout(blocks: &[OcrBlock]) -> LayoutReconstructionResult {
    let valid_blocks: Vec<&OcrBlock> = blocks
        .iter()
        .filter(|b| !b.text.trim().is_empty() && b.bbox.len() == 4)
        .collect();
    if valid_blocks.is_empty() {
        return LayoutReconstructionResult {
            rows: Vec::new(),
            detected_posture: "UNKNOWN".to_owned(),
            detected_date: None,
        };
    }

    let date_re = Regex::new(r"(20\d{2})[-/.](\d{1,2})[-/.](\d{1,2})").unwrap();
    let mut detected_date: Option<String> = None;
    for b in &valid_blocks {
        if let Some(caps) = date_re.captures(&b.text)
            && let (Ok(y), Ok(m), Ok(d)) = (
                caps[1].parse::<u32>(),
                caps[2].parse::<u32>(),
                caps[3].parse::<u32>(),
            )
            && (1..=12).contains(&m)
            && (1..=31).contains(&d)
        {
            detected_date = Some(format!("{y:04}-{m:02}-{d:02}"));
            break;
        }
    }

    let mut heights: Vec<f64> = valid_blocks
        .iter()
        .map(|b| b.height())
        .filter(|&h| h > 0.0)
        .collect();
    heights.sort_by(|a, b| a.total_cmp(b));
    let h_median = if heights.is_empty() {
        24.0
    } else {
        heights[heights.len() / 2]
    };
    let y_threshold = 0.55 * h_median;

    let mut sorted = valid_blocks;
    sorted.sort_by(|a, b| a.y_center().total_cmp(&b.y_center()));

    let mut clustered_rows: Vec<Vec<&OcrBlock>> = Vec::new();
    let mut current_row = vec![sorted[0]];
    let mut current_mean_y = sorted[0].y_center();

    for &block in sorted.iter().skip(1) {
        if (block.y_center() - current_mean_y).abs() < y_threshold {
            current_row.push(block);
            current_mean_y =
                current_row.iter().map(|b| b.y_center()).sum::<f64>() / current_row.len() as f64;
        } else {
            clustered_rows.push(current_row);
            current_row = vec![block];
            current_mean_y = block.y_center();
        }
    }
    clustered_rows.push(current_row);

    let anchor_re = Regex::new(r"^([<>≤≥]?)\s*(\d+(?:\.\d+)?)\s*([↑↓HL+\-]?)$").unwrap();
    let range_re = Regex::new(r"(\d+(?:\.\d+)?\s*[~\-–—]\s*\d+(?:\.\d+)?)").unwrap();
    let digit_re = Regex::new(r"\d").unwrap();

    let mut rows = Vec::new();
    let mut overall_posture = "UNKNOWN".to_owned();

    for mut row in clustered_rows {
        row.sort_by(|a, b| a.x_min().total_cmp(&b.x_min()));

        let mut anchor_idx = None;
        let mut anchor_caps = None;
        for (i, b) in row.iter().enumerate() {
            if let Some(caps) = anchor_re.captures(b.text.trim()) {
                anchor_idx = Some(i);
                anchor_caps = Some(caps);
                break;
            }
        }

        let (Some(idx), Some(caps)) = (anchor_idx, anchor_caps) else {
            for b in &row {
                if let Some(p) = extract_posture(&b.text) {
                    overall_posture = p.to_owned();
                }
            }
            continue;
        };

        if idx == 0 {
            continue;
        }

        let raw_name = row[..idx]
            .iter()
            .map(|b| b.text.trim())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if raw_name.is_empty() {
            continue;
        }

        if let Some(p) = extract_posture(&raw_name) {
            overall_posture = p.to_owned();
        }

        let op_raw = caps.get(1).map_or("", |m| m.as_str());
        let operator_symbol = match op_raw {
            "≤" => "<=",
            "≥" => ">=",
            "<" | ">" => op_raw,
            _ => "=",
        }
        .to_owned();

        let Some(raw_value) = caps.get(2).and_then(|m| m.as_str().parse::<f64>().ok()) else {
            continue;
        };

        let flag_tok = caps.get(3).map_or("", |m| m.as_str()).to_uppercase();
        let abnormal_flag = match flag_tok.as_str() {
            "↑" | "H" | "+" => "HIGH",
            "↓" | "L" | "-" => "LOW",
            _ => "NORMAL",
        }
        .to_owned();

        let mut raw_unit: Option<String> = None;
        let mut ref_range_raw: Option<String> = None;
        for b in row.iter().skip(idx + 1) {
            let t = b.text.trim();
            if t.is_empty() {
                continue;
            }
            let is_range =
                t.contains('~') || range_re.is_match(t) || (t.contains('-') && digit_re.is_match(t));
            if is_range {
                if ref_range_raw.is_none() {
                    ref_range_raw = Some(t.to_owned());
                }
            } else if matches!(t, "↑" | "H" | "↓" | "L") {
                continue;
            } else if raw_unit.is_none() {
                raw_unit = Some(t.to_owned());
            }
        }

        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut score_sum = 0.0;
        for b in &row {
            min_x = min_x.min(b.x_min());
            min_y = min_y.min(b.y_min());
            max_x = max_x.max(b.x_max());
            max_y = max_y.max(b.y_max());
            score_sum += b.score;
        }

        rows.push(ReconstructedRow {
            raw_name,
            operator_symbol,
            raw_value,
            raw_unit,
            ref_range_raw,
            abnormal_flag,
            row_bbox: [
                min_x,
                min_y,
                (max_x - min_x).max(0.0),
                (max_y - min_y).max(0.0),
            ],
            ocr_confidence: score_sum / row.len() as f64,
        });
    }

    LayoutReconstructionResult {
        rows,
        detected_posture: overall_posture,
        detected_date,
    }
}

fn extract_posture(text: &str) -> Option<&'static str> {
    if text.contains("立位") {
        Some("UPRIGHT")
    } else if text.contains("卧位") {
        Some("SUPINE")
    } else if text.contains("坐位") {
        Some("SITTING")
    } else {
        None
    }
}

pub struct EntityMatchResult {
    pub cleaned_name: String,
    pub normalized_unit: Option<String>,
    pub biomarker_code: Option<String>,
    pub display_name: Option<String>,
    pub confidence: f64,
    pub top_candidates: Vec<BiomarkerCandidate>,
}

/// Stage 2: Three-level funnel entity alignment.
pub fn align_entity(
    raw_name: &str,
    raw_unit: Option<&str>,
    dictionary: &DictionaryBundle,
) -> EntityMatchResult {
    let cleaned_name = clean_biomarker_name(raw_name);
    let stripped_name = strip_code_and_posture(raw_name);
    let normalized_unit = normalize_unit(raw_unit);

    if cleaned_name.is_empty() {
        return EntityMatchResult {
            cleaned_name,
            normalized_unit,
            biomarker_code: None,
            display_name: None,
            confidence: 0.0,
            top_candidates: Vec::new(),
        };
    }

    let query_variants: Vec<&str> = if !stripped_name.is_empty() && stripped_name != cleaned_name {
        vec![&cleaned_name, &stripped_name]
    } else {
        vec![&cleaned_name]
    };

    for q in &query_variants {
        if let Some(code) = dictionary.user_mappings.get(*q)
            && let Some(b) = dictionary.biomarkers.get(code)
        {
            return EntityMatchResult {
                cleaned_name,
                normalized_unit,
                biomarker_code: Some(b.code.clone()),
                display_name: Some(b.display_name.clone()),
                confidence: 1.0,
                top_candidates: vec![BiomarkerCandidate {
                    code: b.code.clone(),
                    display_name: b.display_name.clone(),
                    score: 1.0,
                }],
            };
        }
    }

    for q in &query_variants {
        if let Some(code) = dictionary.system_aliases.get(*q)
            && let Some(b) = dictionary.biomarkers.get(code)
        {
            return EntityMatchResult {
                cleaned_name,
                normalized_unit,
                biomarker_code: Some(b.code.clone()),
                display_name: Some(b.display_name.clone()),
                confidence: 0.98,
                top_candidates: vec![BiomarkerCandidate {
                    code: b.code.clone(),
                    display_name: b.display_name.clone(),
                    score: 0.98,
                }],
            };
        }
    }

    let mut candidates = Vec::with_capacity(dictionary.biomarkers.len());
    for (code, biomarker) in &dictionary.biomarkers {
        let aliases = dictionary
            .aliases_by_code
            .get(code)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let mut best_text_score = 0.0_f64;
        for q in &query_variants {
            for alias in aliases {
                best_text_score = best_text_score.max(combined_text_score(q, alias));
            }
        }

        let unit_matched = normalized_unit.as_ref().is_some_and(|nu| {
            biomarker
                .valid_units
                .iter()
                .any(|u| u.eq_ignore_ascii_case(nu))
        });

        let final_score = if unit_matched {
            (best_text_score + 0.15).min(1.0)
        } else {
            best_text_score
        };

        candidates.push(BiomarkerCandidate {
            code: code.clone(),
            display_name: biomarker.display_name.clone(),
            score: final_score,
        });
    }

    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    let top3: Vec<BiomarkerCandidate> = candidates.into_iter().take(3).collect();

    if top3.is_empty() || top3[0].score < ACCEPT_THRESHOLD {
        let conf = top3.first().map_or(0.0, |c| c.score);
        return EntityMatchResult {
            cleaned_name,
            normalized_unit,
            biomarker_code: None,
            display_name: None,
            confidence: conf,
            top_candidates: top3,
        };
    }

    EntityMatchResult {
        cleaned_name,
        normalized_unit,
        biomarker_code: Some(top3[0].code.clone()),
        display_name: Some(top3[0].display_name.clone()),
        confidence: top3[0].score,
        top_candidates: top3,
    }
}

/// Stage 3: Deterministic unit conversion.
pub fn convert_unit(
    raw_value: f64,
    raw_unit: Option<&str>,
    biomarker: Option<&StandardBiomarker>,
) -> Option<f64> {
    let biomarker = biomarker?;
    let Some(unit) = normalize_unit(raw_unit) else {
        return Some(raw_value);
    };

    let factor = biomarker
        .conversions
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(&unit))
        .map_or(1.0, |(_, &v)| v);

    Some(raw_value * factor)
}

/// Full Stage 1 -> Stage 3 pipeline.
pub fn parse_ocr_response(
    ocr: &OcrResponse,
    dictionary: &DictionaryBundle,
) -> (Vec<ParsedLabItem>, String, Option<String>) {
    let layout = reconstruct_layout(&ocr.blocks);
    let mut items = Vec::with_capacity(layout.rows.len());

    for row in layout.rows {
        let matched = align_entity(&row.raw_name, row.raw_unit.as_deref(), dictionary);
        let biomarker = matched
            .biomarker_code
            .as_deref()
            .and_then(|c| dictionary.biomarkers.get(c));

        let effective_unit = matched
            .normalized_unit
            .clone()
            .or_else(|| row.raw_unit.clone());

        let normalized_value =
            convert_unit(row.raw_value, effective_unit.as_deref(), biomarker);

        let combined_conf = (matched.confidence * 0.8 + row.ocr_confidence * 0.2).clamp(0.0, 1.0);
        let confidence = if matched.biomarker_code.is_some() {
            combined_conf
        } else {
            matched.confidence
        };

        items.push(ParsedLabItem {
            result_id: Uuid::new_v4().to_string(),
            biomarker_code: matched.biomarker_code,
            display_name: matched.display_name,
            raw_name: row.raw_name,
            cleaned_name: matched.cleaned_name,
            operator_symbol: row.operator_symbol,
            raw_value: row.raw_value,
            raw_unit: effective_unit,
            normalized_value,
            standard_unit: biomarker.map(|b| b.standard_unit.clone()),
            ref_range_raw: row.ref_range_raw,
            abnormal_flag: row.abnormal_flag,
            row_bbox: row.row_bbox,
            confidence,
            top_candidates: matched.top_candidates,
            is_user_confirmed: matched.confidence >= 0.85,
        });
    }

    (items, layout.detected_posture, layout.detected_date)
}

/// Built-in demo OCR payload for instant local Linux verification.
pub fn demo_ocr_response() -> OcrResponse {
    serde_json::from_str(
        r#"{
          "imageWidth": 1080,
          "imageHeight": 1920,
          "blocks": [
            {"text": "血浆醛固酮(立位)", "score": 0.96, "box": [[80, 320], [360, 320], [360, 355], [80, 355]]},
            {"text": "225.0 ↑", "score": 0.98, "box": [[440, 322], [550, 322], [550, 354], [440, 354]]},
            {"text": "pg/ml", "score": 0.94, "box": [[600, 322], [690, 322], [690, 354], [600, 354]]},
            {"text": "30-160", "score": 0.95, "box": [[750, 322], [880, 322], [880, 354], [750, 354]]},
            {"text": "直接肾素浓度", "score": 0.92, "box": [[80, 390], [310, 390], [310, 425], [80, 425]]},
            {"text": "4.2 ↓", "score": 0.97, "box": [[440, 392], [520, 392], [520, 424], [440, 424]]},
            {"text": "mlU/L", "score": 0.91, "box": [[600, 392], [690, 392], [690, 424], [600, 424]]},
            {"text": "血清钾测定", "score": 0.95, "box": [[80, 460], [280, 460], [280, 495], [80, 495]]},
            {"text": "3.45 ↓", "score": 0.99, "box": [[440, 462], [530, 462], [530, 494], [440, 494]]},
            {"text": "mmol/l", "score": 0.95, "box": [[600, 462], [700, 462], [700, 494], [600, 494]]},
            {"text": "3.50-5.30", "score": 0.96, "box": [[750, 462], [900, 462], [900, 494], [750, 494]]},
            {"text": "血浆肾素活性", "score": 0.94, "box": [[80, 530], [290, 530], [290, 565], [80, 565]]},
            {"text": "0.45", "score": 0.96, "box": [[440, 532], [520, 532], [520, 564], [440, 564]]},
            {"text": "ng/ml/h", "score": 0.93, "box": [[600, 532], [700, 532], [700, 564], [600, 564]]},
            {"text": "未知肌酐项目X", "score": 0.85, "box": [[80, 600], [290, 600], [290, 635], [80, 635]]},
            {"text": "1.2", "score": 0.94, "box": [[440, 602], [520, 602], [520, 634], [440, 634]]},
            {"text": "mg/dl", "score": 0.90, "box": [[600, 602], [690, 602], [690, 634], [600, 634]]}
          ]
        }"#,
    )
    .expect("demo OCR JSON must be valid")
}
