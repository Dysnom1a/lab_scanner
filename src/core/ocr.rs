use std::path::{Path, PathBuf};
use std::process::Command;

use super::models::{OcrBlock, OcrResponse};

const RAPIDOCR_SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/rapidocr_infer.py");

fn rapidocr_python_path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    let venv_py = home.join(".local/share/lab_scanner_nsh/ocr_venv/bin/python");
    if venv_py.exists() && Path::new(RAPIDOCR_SCRIPT).exists() {
        Some(venv_py)
    } else {
        None
    }
}

/// Runs local offline OCR on [image_path], preferring RapidOCR (PP-OCRv4)
/// when installed, and falling back to Tesseract TSV.
pub fn recognize_local_image(image_path: &Path) -> Result<(OcrResponse, String), String> {
    if !image_path.exists() {
        return Err(format!("图片文件不存在: {}", image_path.display()));
    }

    if let Some(py) = rapidocr_python_path() {
        return recognize_with_rapidocr(&py, image_path);
    }

    recognize_image_with_tesseract(image_path)
}

fn recognize_with_rapidocr(
    python_bin: &Path,
    image_path: &Path,
) -> Result<(OcrResponse, String), String> {
    let output = Command::new(python_bin)
        .arg(RAPIDOCR_SCRIPT)
        .arg(image_path)
        .output()
        .map_err(|e| format!("启动 RapidOCR 失败: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("RapidOCR 识别失败: {err}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let ocr: OcrResponse =
        serde_json::from_str(&stdout).map_err(|e| format!("解析 RapidOCR 输出失败: {e}"))?;
    let notice = format!(
        "RapidOCR (PP-OCRv4 中文+数字) 识别完成，提取 {} 个文本块",
        ocr.blocks.len()
    );
    Ok((ocr, notice))
}

/// Fallback: runs local Tesseract OCR on [image_path] and converts TSV word boxes
/// into the unified `OcrResponse` contract.
pub fn recognize_image_with_tesseract(image_path: &Path) -> Result<(OcrResponse, String), String> {
    if !image_path.exists() {
        return Err(format!("图片文件不存在: {}", image_path.display()));
    }

    let langs_output = Command::new("tesseract")
        .arg("--list-langs")
        .output()
        .map_err(|e| format!("无法启动 tesseract（请先安装 tesseract）: {e}"))?;

    let langs_str = String::from_utf8_lossy(&langs_output.stdout);
    let has_chi_sim = langs_str.lines().any(|l| l.trim() == "chi_sim");
    let lang_arg = if has_chi_sim { "chi_sim+eng" } else { "eng" };

    let output = Command::new("tesseract")
        .arg(image_path)
        .arg("stdout")
        .arg("-l")
        .arg(lang_arg)
        .arg("tsv")
        .output()
        .map_err(|e| format!("执行 tesseract 失败: {e}"))?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(format!("tesseract 识别失败: {err}"));
    }

    let tsv = String::from_utf8_lossy(&output.stdout);
    let mut blocks = Vec::new();
    let mut max_x = 0.0_f64;
    let mut max_y = 0.0_f64;

    for (idx, line) in tsv.lines().enumerate() {
        if idx == 0 {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 12 || cols[0] != "5" {
            continue;
        }
        let text = cols[11].trim();
        if text.is_empty() {
            continue;
        }

        let left: f64 = cols[6].parse().unwrap_or(0.0);
        let top: f64 = cols[7].parse().unwrap_or(0.0);
        let width: f64 = cols[8].parse().unwrap_or(0.0);
        let height: f64 = cols[9].parse().unwrap_or(0.0);
        let conf: f64 = cols[10].parse().unwrap_or(0.0);
        if conf < 0.0 || width <= 0.0 || height <= 0.0 {
            continue;
        }

        max_x = max_x.max(left + width);
        max_y = max_y.max(top + height);

        blocks.push(OcrBlock {
            text: text.to_owned(),
            score: (conf / 100.0).clamp(0.0, 1.0),
            bbox: vec![
                [left, top],
                [left + width, top],
                [left + width, top + height],
                [left, top + height],
            ],
        });
    }

    let notice = if has_chi_sim {
        format!("Tesseract ({lang_arg}) 识别完成，提取 {} 个文本块", blocks.len())
    } else {
        format!(
            "Tesseract (eng) 识别完成，提取 {} 个文本块（未检测到 chi_sim 中文包，建议安装 tesseract-data-chi_sim）",
            blocks.len()
        )
    };

    Ok((
        OcrResponse {
            image_width: max_x.ceil() as u32,
            image_height: max_y.ceil() as u32,
            blocks,
        },
        notice,
    ))
}
