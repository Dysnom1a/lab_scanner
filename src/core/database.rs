use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, Result as SqlResult, params};
use serde::Deserialize;

use super::models::{
    DictionaryBundle, LabRecord, ParsedLabItem, StandardBiomarker, TrendMatrixData, TrendPoint,
};
use super::normalizer::clean_biomarker_name;

const SEED_JSON: &str = include_str!("../../assets/seed_biomarkers.json");

#[derive(Deserialize)]
struct SeedBiomarker {
    code: String,
    display_name: String,
    category: String,
    standard_unit: String,
    valid_units: Vec<String>,
    conversions: HashMap<String, f64>,
    #[serde(default)]
    aliases: Vec<String>,
}

pub struct AppDatabase {
    conn: Connection,
}

impl AppDatabase {
    pub fn default_db_path() -> PathBuf {
        if let Some(dir) = nsh_core::prelude::data_dir() {
            return dir.join("lab_scanner.db");
        }
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
            })
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("lab_scanner_nsh").join("lab_scanner.db")
    }

    pub fn open_default() -> SqlResult<Self> {
        let path = Self::default_db_path();
        Self::open_path(&path)
    }

    pub fn open_path(path: &Path) -> SqlResult<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> SqlResult<Self> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> SqlResult<Self> {
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;",
        )?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS standard_biomarkers (
                code TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                category TEXT NOT NULL,
                standard_unit TEXT NOT NULL,
                valid_units_json TEXT NOT NULL,
                conversions_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS biomarker_aliases (
                alias_cleaned TEXT PRIMARY KEY,
                biomarker_code TEXT NOT NULL,
                FOREIGN KEY (biomarker_code) REFERENCES standard_biomarkers(code) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS user_custom_mappings (
                raw_cleaned_name TEXT PRIMARY KEY,
                biomarker_code TEXT NOT NULL,
                updated_at INTEGER NOT NULL,
                FOREIGN KEY (biomarker_code) REFERENCES standard_biomarkers(code) ON DELETE CASCADE
            );
            CREATE TABLE IF NOT EXISTS lab_records (
                record_id TEXT PRIMARY KEY,
                test_date TEXT NOT NULL,
                hospital_name TEXT,
                posture TEXT DEFAULT 'UNKNOWN',
                medication_tag TEXT,
                image_path TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS lab_results (
                result_id TEXT PRIMARY KEY,
                record_id TEXT NOT NULL,
                biomarker_code TEXT,
                raw_name TEXT NOT NULL,
                operator TEXT DEFAULT '=',
                raw_value REAL NOT NULL,
                raw_unit TEXT,
                normalized_value REAL,
                ref_range_raw TEXT,
                abnormal_flag TEXT DEFAULT 'NORMAL',
                row_bbox_json TEXT,
                confidence REAL NOT NULL,
                FOREIGN KEY (record_id) REFERENCES lab_records(record_id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS idx_results_code_date ON lab_results(biomarker_code);
            CREATE INDEX IF NOT EXISTS idx_records_date ON lab_records(test_date);",
        )?;

        let mut db = Self { conn };
        db.seed_if_empty()?;
        Ok(db)
    }

    fn seed_if_empty(&mut self) -> SqlResult<()> {
        let items: Vec<SeedBiomarker> = serde_json::from_str(SEED_JSON)
            .expect("seed_biomarkers.json must be valid JSON");

        let tx = self.conn.transaction()?;
        {
            let mut stmt_bio = tx.prepare(
                "INSERT OR IGNORE INTO standard_biomarkers
                 (code, display_name, category, standard_unit, valid_units_json, conversions_json)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            let mut stmt_alias = tx.prepare(
                "INSERT OR IGNORE INTO biomarker_aliases (alias_cleaned, biomarker_code)
                 VALUES (?1, ?2)",
            )?;

            for item in items {
                let valid_units_json = serde_json::to_string(&item.valid_units).unwrap();
                let conversions_json = serde_json::to_string(&item.conversions).unwrap();
                stmt_bio.execute(params![
                    item.code,
                    item.display_name,
                    item.category,
                    item.standard_unit,
                    valid_units_json,
                    conversions_json
                ])?;

                let mut aliases = BTreeSet::new();
                aliases.insert(clean_biomarker_name(&item.code));
                aliases.insert(clean_biomarker_name(&item.display_name));
                for a in &item.aliases {
                    aliases.insert(clean_biomarker_name(a));
                }
                for alias in aliases {
                    if !alias.is_empty() {
                        stmt_alias.execute(params![alias, item.code])?;
                    }
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_dictionary_bundle(&self) -> SqlResult<DictionaryBundle> {
        let mut biomarkers = HashMap::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT code, display_name, category, standard_unit, valid_units_json, conversions_json
                 FROM standard_biomarkers",
            )?;
            let rows = stmt.query_map([], |row| {
                let valid_units_json: String = row.get(4)?;
                let conversions_json: String = row.get(5)?;
                let valid_units: Vec<String> =
                    serde_json::from_str(&valid_units_json).unwrap_or_default();
                let conversions: HashMap<String, f64> =
                    serde_json::from_str(&conversions_json).unwrap_or_default();
                Ok(StandardBiomarker {
                    code: row.get(0)?,
                    display_name: row.get(1)?,
                    category: row.get(2)?,
                    standard_unit: row.get(3)?,
                    valid_units,
                    conversions,
                })
            })?;
            for b in rows {
                let b = b?;
                biomarkers.insert(b.code.clone(), b);
            }
        }

        let mut system_aliases = HashMap::new();
        let mut aliases_by_code: HashMap<String, Vec<String>> = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT alias_cleaned, biomarker_code FROM biomarker_aliases")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for r in rows {
                let (alias, code) = r?;
                system_aliases.insert(alias.clone(), code.clone());
                aliases_by_code.entry(code).or_default().push(alias);
            }
        }

        let mut user_mappings = HashMap::new();
        {
            let mut stmt = self
                .conn
                .prepare("SELECT raw_cleaned_name, biomarker_code FROM user_custom_mappings")?;
            let rows = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for r in rows {
                let (raw_cleaned, code) = r?;
                user_mappings.insert(raw_cleaned, code);
            }
        }

        Ok(DictionaryBundle {
            biomarkers,
            system_aliases,
            user_mappings,
            aliases_by_code,
        })
    }

    pub fn save_lab_record_with_results(
        &mut self,
        record: &LabRecord,
        items: &[ParsedLabItem],
    ) -> SqlResult<()> {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);

        let tx = self.conn.transaction()?;
        {
            tx.execute(
                "INSERT OR REPLACE INTO lab_records
                 (record_id, test_date, hospital_name, posture, medication_tag, image_path, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    record.record_id,
                    record.test_date,
                    record.hospital_name,
                    record.posture,
                    record.medication_tag,
                    record.image_path,
                    record.created_at
                ],
            )?;

            let mut stmt_res = tx.prepare(
                "INSERT OR REPLACE INTO lab_results
                 (result_id, record_id, biomarker_code, raw_name, operator, raw_value, raw_unit,
                  normalized_value, ref_range_raw, abnormal_flag, row_bbox_json, confidence)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;

            let mut stmt_map = tx.prepare(
                "INSERT OR REPLACE INTO user_custom_mappings
                 (raw_cleaned_name, biomarker_code, updated_at)
                 VALUES (?1, ?2, ?3)",
            )?;

            for item in items {
                let bbox_json = serde_json::to_string(&item.row_bbox).unwrap_or_default();
                stmt_res.execute(params![
                    item.result_id,
                    record.record_id,
                    item.biomarker_code,
                    item.raw_name,
                    item.operator_symbol,
                    item.raw_value,
                    item.raw_unit,
                    item.normalized_value,
                    item.ref_range_raw,
                    item.abnormal_flag,
                    bbox_json,
                    item.confidence
                ])?;

                if item.is_user_confirmed
                    && let Some(code) = &item.biomarker_code
                    && !item.cleaned_name.is_empty()
                {
                    stmt_map.execute(params![item.cleaned_name, code, now_ms])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_trend_matrix(&self) -> SqlResult<TrendMatrixData> {
        let mut biomarkers = Vec::new();
        {
            let mut stmt = self.conn.prepare(
                "SELECT code, display_name, category, standard_unit, valid_units_json, conversions_json
                 FROM standard_biomarkers
                 ORDER BY category ASC, code ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let valid_units_json: String = row.get(4)?;
                let conversions_json: String = row.get(5)?;
                Ok(StandardBiomarker {
                    code: row.get(0)?,
                    display_name: row.get(1)?,
                    category: row.get(2)?,
                    standard_unit: row.get(3)?,
                    valid_units: serde_json::from_str(&valid_units_json).unwrap_or_default(),
                    conversions: serde_json::from_str(&conversions_json).unwrap_or_default(),
                })
            })?;
            for b in rows {
                biomarkers.push(b?);
            }
        }

        let mut dates_set = BTreeSet::new();
        let mut points_by_code_and_date: HashMap<String, HashMap<String, TrendPoint>> =
            HashMap::new();
        let mut series_by_code: HashMap<String, Vec<TrendPoint>> = HashMap::new();

        {
            let mut stmt = self.conn.prepare(
                "SELECT
                    r.record_id,
                    r.test_date,
                    r.posture,
                    res.biomarker_code,
                    res.raw_value,
                    res.raw_unit,
                    COALESCE(res.normalized_value, res.raw_value) AS effective_value,
                    res.abnormal_flag
                 FROM lab_results res
                 INNER JOIN lab_records r ON res.record_id = r.record_id
                 WHERE res.biomarker_code IS NOT NULL
                 ORDER BY r.test_date ASC, r.created_at ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(3)?,
                    TrendPoint {
                        record_id: row.get(0)?,
                        test_date: row.get(1)?,
                        posture: row
                            .get::<_, Option<String>>(2)?
                            .unwrap_or_else(|| "UNKNOWN".to_owned()),
                        raw_value: row.get(4)?,
                        raw_unit: row.get(5)?,
                        normalized_value: row.get(6)?,
                        abnormal_flag: row
                            .get::<_, Option<String>>(7)?
                            .unwrap_or_else(|| "NORMAL".to_owned()),
                    },
                ))
            })?;

            for r in rows {
                let (code, point) = r?;
                dates_set.insert(point.test_date.clone());
                points_by_code_and_date
                    .entry(code.clone())
                    .or_default()
                    .insert(point.test_date.clone(), point.clone());
                series_by_code.entry(code).or_default().push(point);
            }
        }

        Ok(TrendMatrixData {
            dates: dates_set.into_iter().collect(),
            biomarkers,
            points_by_code_and_date,
            series_by_code,
        })
    }
}
