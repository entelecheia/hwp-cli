//! `hwp validate` — 구조 검증.
//!
//! mimetype·필수 엔트리 존재·XML 파싱(본문/헤더)을 확인한다. XSD 스키마 검증은
//! 범위 밖. 하드 오류가 하나라도 있으면 비-0 종료코드(소비자가 exit code로 판정).

use std::path::Path;

use crate::format::{FileFormat, detect};

pub fn run(path: &Path, json: bool) -> anyhow::Result<()> {
    let mut errors: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut format = "unknown";

    match detect(path) {
        Ok(FileFormat::Hwpx) => {
            format = "hwpx";
            validate_hwpx(path, &mut errors, &mut warnings);
        }
        Ok(FileFormat::Hwp5) => {
            format = "hwp5";
            match hwp5::read_document(path) {
                Ok(r) => warnings.extend(r.warnings),
                Err(e) => errors.push(format!("파싱 실패: {e}")),
            }
        }
        Err(e) => errors.push(format!("포맷 감지 실패: {e}")),
    }

    let valid = errors.is_empty();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "file": path.display().to_string(),
                "format": format,
                "valid": valid,
                "errors": errors,
                "warnings": warnings,
            }))?
        );
    } else {
        println!("파일: {}", path.display());
        println!("포맷: {format}");
        println!("결과: {}", if valid { "유효" } else { "오류" });
        for e in &errors {
            println!("  오류: {e}");
        }
        for w in &warnings {
            println!("  경고: {w}");
        }
    }

    if valid {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn validate_hwpx(path: &Path, errors: &mut Vec<String>, warnings: &mut Vec<String>) {
    let mut pkg = match hwpx::HwpxPackage::open(path) {
        Ok(p) => p,
        Err(e) => {
            errors.push(format!("패키지/mimetype 오류: {e}"));
            return;
        }
    };

    let names: Vec<String> = match pkg.entries() {
        Ok(es) => es.into_iter().map(|e| e.name).collect(),
        Err(e) => {
            errors.push(format!("엔트리 목록 실패: {e}"));
            return;
        }
    };
    for req in [
        "mimetype",
        "version.xml",
        "Contents/header.xml",
        "META-INF/container.xml",
    ] {
        if !names.iter().any(|n| n == req) {
            errors.push(format!("필수 엔트리 누락: {req}"));
        }
    }
    if !names.iter().any(|n| n.starts_with("Contents/section")) {
        errors.push("본문 섹션(Contents/section*.xml) 없음".to_string());
    }

    match hwpx::read_document(path) {
        Ok(r) => warnings.extend(r.warnings),
        Err(e) => errors.push(format!("XML 파싱 실패: {e}")),
    }
}
