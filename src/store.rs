use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

use nsh_core::file_dialog::{FileFilter, OpenDialog};
use nsh_core::prelude::*;
use uuid::Uuid;

use crate::core::database::AppDatabase;
use crate::core::models::{
    DictionaryBundle, LabRecord, OcrResponse, ParsedLabItem, TrendMatrixData,
};
use crate::core::ocr::recognize_local_image;
use crate::core::pipeline::{convert_unit, demo_ocr_response, parse_ocr_response};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Review,
    Dashboard,
}

#[derive(Clone)]
pub struct AppStore {
    db: Rc<RefCell<AppDatabase>>,
    pub active_tab: Signal<Tab>,
    pub test_date: Signal<String>,
    pub posture: Signal<String>,
    pub hospital: Signal<String>,
    pub ocr_json_path: Signal<String>,
    pub items: Signal<Vec<ParsedLabItem>>,
    pub status_message: Signal<String>,
    pub trend_matrix: Signal<TrendMatrixData>,
    pub dictionary: Signal<Option<DictionaryBundle>>,
    pub expanded_ids: Signal<HashSet<String>>,
    pub selected_codes: Signal<HashSet<String>>,
}

impl AppStore {
    pub fn open_default() -> Self {
        let db = AppDatabase::open_default()
            .or_else(|_| AppDatabase::open_in_memory())
            .expect("open SQLite database");
        Self::from_db(db)
    }

    pub fn open_in_memory() -> Self {
        let db = AppDatabase::open_in_memory().expect("open in-memory SQLite database");
        Self::from_db(db)
    }

    fn from_db(db: AppDatabase) -> Self {
        let dict = db.load_dictionary_bundle().ok();
        let matrix = db.get_trend_matrix().unwrap_or_default();
        Self {
            db: Rc::new(RefCell::new(db)),
            active_tab: signal(Tab::Review),
            test_date: signal("2026-11-15".to_owned()),
            posture: signal("UNKNOWN".to_owned()),
            hospital: signal(String::new()),
            ocr_json_path: signal(String::new()),
            items: signal(Vec::new()),
            status_message: signal("就绪：可加载模拟化验单或本地 OCR JSON 文件".to_owned()),
            trend_matrix: signal(matrix),
            dictionary: signal(dict),
            expanded_ids: signal(HashSet::new()),
            selected_codes: signal(HashSet::from([
                "PAC".to_owned(),
                "PRA".to_owned(),
                "DRC".to_owned(),
            ])),
        }
    }

    pub fn toggle_expanded(&self, id: &str) {
        self.expanded_ids.update(|set| {
            if !set.remove(id) {
                set.insert(id.to_owned());
            }
        });
    }

    pub fn toggle_selected_code(&self, code: &str) {
        self.selected_codes.update(|set| {
            if !set.remove(code) {
                set.insert(code.to_owned());
            }
        });
    }

    pub fn load_demo(&self) {
        let ocr = demo_ocr_response();
        self.apply_ocr(&ocr, "demo://lab_report");
        self.status_message
            .set("已加载模拟化验单（含 1 项低置信度项目待核对）".to_owned());
    }

    pub fn pick_and_recognize_local_file(&self) {
        let store = self.clone();
        spawn(async move {
            let dialog = OpenDialog::files().filter(FileFilter::new(
                "化验单图片 / OCR JSON",
                ["png", "jpg", "jpeg", "webp", "bmp", "json"],
            ));
            match pick_paths(dialog).await {
                Ok(Some(paths)) => {
                    if let Some(first) = paths.into_iter().next() {
                        let path_str = first.to_string_lossy().into_owned();
                        store.ocr_json_path.set(path_str);
                        store.load_local_file();
                    }
                }
                Ok(None) => {
                    store.status_message.set("已取消选择文件".to_owned());
                }
                Err(e) => {
                    store
                        .status_message
                        .set(format!("无法打开系统文件选择器: {e}"));
                }
            }
        });
    }

    pub fn load_local_file(&self) {
        let path_str = self.ocr_json_path.get_untracked().trim().to_owned();
        if path_str.is_empty() {
            self.status_message
                .set("请先选择本地图片或输入 OCR JSON 路径".to_owned());
            return;
        }

        let path = Path::new(&path_str);
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        if ext == "json" {
            match std::fs::read_to_string(path) {
                Ok(content) => match serde_json::from_str::<OcrResponse>(&content) {
                    Ok(ocr) => {
                        self.apply_ocr(&ocr, &path_str);
                        self.status_message
                            .set(format!("已解析本地 OCR JSON: {path_str}"));
                    }
                    Err(e) => {
                        self.status_message.set(format!("JSON 解析失败: {e}"));
                    }
                },
                Err(e) => {
                    self.status_message.set(format!("读取文件失败: {e}"));
                }
            }
        } else {
            self.status_message
                .set(format!("正在使用离线 OCR 识别图片: {path_str} ..."));
            match recognize_local_image(path) {
                Ok((ocr, notice)) => {
                    self.apply_ocr(&ocr, &path_str);
                    self.status_message.set(notice);
                }
                Err(e) => {
                    self.status_message.set(e);
                }
            }
        }
    }

    fn apply_ocr(&self, ocr: &OcrResponse, _source: &str) {
        let Ok(dict) = self.db.borrow().load_dictionary_bundle() else {
            self.status_message.set("加载指标字典失败".to_owned());
            return;
        };
        let (parsed, detected_posture, detected_date) = parse_ocr_response(ocr, &dict);
        let default_expanded: HashSet<String> = parsed
            .iter()
            .filter(|it| it.confidence < 0.85 || it.biomarker_code.is_none())
            .map(|it| it.result_id.clone())
            .collect();
        self.dictionary.set(Some(dict));
        self.expanded_ids.set(default_expanded);
        self.items.set(parsed);
        self.posture.set(detected_posture);
        if let Some(date) = detected_date {
            self.test_date.set(date);
        }
    }

    pub fn bind_item_biomarker(&self, index: usize, code: &str) {
        let dict = self.dictionary.get_untracked();
        let Some(dict) = dict else { return };
        let Some(biomarker) = dict.biomarkers.get(code) else {
            return;
        };

        self.items.update(|list| {
            if let Some(item) = list.get_mut(index) {
                item.biomarker_code = Some(biomarker.code.clone());
                item.display_name = Some(biomarker.display_name.clone());
                item.standard_unit = Some(biomarker.standard_unit.clone());
                item.normalized_value = convert_unit(
                    item.raw_value,
                    item.raw_unit.as_deref(),
                    Some(biomarker),
                );
                item.is_user_confirmed = true;
            }
        });
    }

    pub fn update_item_value(&self, index: usize, value_str: &str) {
        let Ok(parsed) = value_str.trim().parse::<f64>() else {
            return;
        };
        let dict = self.dictionary.get_untracked();
        self.items.update(|list| {
            if let Some(item) = list.get_mut(index) {
                item.raw_value = parsed;
                let biomarker = dict.as_ref().and_then(|d| {
                    item.biomarker_code
                        .as_deref()
                        .and_then(|c| d.biomarkers.get(c))
                });
                item.normalized_value =
                    convert_unit(parsed, item.raw_unit.as_deref(), biomarker);
                item.is_user_confirmed = true;
            }
        });
    }

    pub fn update_item_unit(&self, index: usize, unit_str: &str) {
        let dict = self.dictionary.get_untracked();
        let trimmed = unit_str.trim();
        let next_unit = if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_owned())
        };
        self.items.update(|list| {
            if let Some(item) = list.get_mut(index) {
                item.raw_unit = next_unit.clone();
                let biomarker = dict.as_ref().and_then(|d| {
                    item.biomarker_code
                        .as_deref()
                        .and_then(|c| d.biomarkers.get(c))
                });
                item.normalized_value =
                    convert_unit(item.raw_value, item.raw_unit.as_deref(), biomarker);
                item.is_user_confirmed = true;
            }
        });
    }

    pub fn save_current_record(&self) {
        let current_items = self.items.get_untracked();
        if current_items.is_empty() {
            self.status_message.set("当前没有可保存的化验项".to_owned());
            return;
        }

        let date = self.test_date.get_untracked().trim().to_owned();
        let hospital = self.hospital.get_untracked().trim().to_owned();
        let image_path = self.ocr_json_path.get_untracked().trim().to_owned();
        let record = LabRecord {
            record_id: Uuid::new_v4().to_string(),
            test_date: if date.is_empty() {
                "2026-11-15".to_owned()
            } else {
                date
            },
            hospital_name: if hospital.is_empty() {
                None
            } else {
                Some(hospital)
            },
            posture: self.posture.get_untracked(),
            medication_tag: None,
            image_path: if image_path.is_empty() {
                "demo://lab_report".to_owned()
            } else {
                image_path
            },
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0),
        };

        let res = {
            let mut db = self.db.borrow_mut();
            db.save_lab_record_with_results(&record, &current_items)
        };

        match res {
            Ok(()) => {
                if let Ok(matrix) = self.db.borrow().get_trend_matrix() {
                    self.trend_matrix.set(matrix);
                }
                if let Ok(dict) = self.db.borrow().load_dictionary_bundle() {
                    self.dictionary.set(Some(dict));
                }
                self.status_message
                    .set("已事务保存至 SQLite 并更新个人映射记忆".to_owned());
            }
            Err(e) => {
                self.status_message.set(format!("保存失败: {e}"));
            }
        }
    }
}
