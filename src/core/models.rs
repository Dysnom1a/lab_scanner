use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OcrBlock {
    pub text: String,
    pub score: f64,
    #[serde(rename = "box")]
    pub bbox: Vec<[f64; 2]>,
}

impl OcrBlock {
    pub fn y_center(&self) -> f64 {
        if self.bbox.len() < 4 {
            return 0.0;
        }
        (self.bbox[0][1] + self.bbox[2][1]) / 2.0
    }

    pub fn x_min(&self) -> f64 {
        if self.bbox.len() < 4 {
            return 0.0;
        }
        self.bbox[0][0].min(self.bbox[3][0])
    }

    pub fn x_max(&self) -> f64 {
        if self.bbox.len() < 4 {
            return 0.0;
        }
        self.bbox[1][0].max(self.bbox[2][0])
    }

    pub fn y_min(&self) -> f64 {
        if self.bbox.len() < 4 {
            return 0.0;
        }
        self.bbox[0][1].min(self.bbox[1][1])
    }

    pub fn y_max(&self) -> f64 {
        if self.bbox.len() < 4 {
            return 0.0;
        }
        self.bbox[2][1].max(self.bbox[3][1])
    }

    pub fn height(&self) -> f64 {
        if self.bbox.len() < 4 {
            return 0.0;
        }
        (self.bbox[2][1] - self.bbox[0][1]).abs()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrResponse {
    pub image_width: u32,
    pub image_height: u32,
    pub blocks: Vec<OcrBlock>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StandardBiomarker {
    pub code: String,
    pub display_name: String,
    pub category: String,
    pub standard_unit: String,
    pub valid_units: Vec<String>,
    pub conversions: HashMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BiomarkerCandidate {
    pub code: String,
    pub display_name: String,
    pub score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLabItem {
    pub result_id: String,
    pub biomarker_code: Option<String>,
    pub display_name: Option<String>,
    pub raw_name: String,
    pub cleaned_name: String,
    pub operator_symbol: String,
    pub raw_value: f64,
    pub raw_unit: Option<String>,
    pub normalized_value: Option<f64>,
    pub standard_unit: Option<String>,
    pub ref_range_raw: Option<String>,
    pub abnormal_flag: String,
    pub row_bbox: [f64; 4],
    pub confidence: f64,
    pub top_candidates: Vec<BiomarkerCandidate>,
    pub is_user_confirmed: bool,
}

#[derive(Debug, Clone)]
pub struct LabRecord {
    pub record_id: String,
    pub test_date: String,
    pub hospital_name: Option<String>,
    pub posture: String,
    pub medication_tag: Option<String>,
    pub image_path: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DictionaryBundle {
    pub biomarkers: HashMap<String, StandardBiomarker>,
    pub system_aliases: HashMap<String, String>,
    pub user_mappings: HashMap<String, String>,
    pub aliases_by_code: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TrendPoint {
    pub record_id: String,
    pub test_date: String,
    pub posture: String,
    pub raw_value: f64,
    pub raw_unit: Option<String>,
    pub normalized_value: f64,
    pub abnormal_flag: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrendMatrixData {
    pub dates: Vec<String>,
    pub biomarkers: Vec<StandardBiomarker>,
    pub points_by_code_and_date: HashMap<String, HashMap<String, TrendPoint>>,
    pub series_by_code: HashMap<String, Vec<TrendPoint>>,
}
