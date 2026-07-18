//! 신규 생성 동등성 테스트 — fixtures/samples/report-tables.hwpx를 JSON IR 경로로
//! 재생성(convert → json → new --from json)해 원본과 동등한지 검증한다(픽스처 하드 의존).
//!
//! 게이트: ① convert/new/validate 정상 종료 ② `hwp cat` stdout 전문 동일 ③ 표 지도
//! (개수·rows/cols·span 멀티셋·셀 width) 동일 ④ secPr 슬라이스 바이트 동일(Gap B)
//! ⑤ tabProperties 슬라이스 바이트 동일(Gap C). linesegarray·PrvImage·settings는
//! 설계상 재생성 차이(한글 재계산/범위 외)로 검증하지 않는다.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Command;

fn hwp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hwp"))
}

fn fixture() -> PathBuf {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/samples/report-tables.hwpx");
    assert!(p.exists(), "커밋된 픽스처 없음: {}", p.display());
    p
}

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("hwp-cli-regen");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

fn cat(path: &PathBuf) -> String {
    let out = hwp().arg("cat").arg(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn zip_slice(path: &PathBuf, entry: &str, open: &str, close: &str) -> String {
    let mut zip = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut s = String::new();
    zip.by_name(entry).unwrap().read_to_string(&mut s).unwrap();
    let i = s
        .find(open)
        .unwrap_or_else(|| panic!("{open} 없음 ({entry})"));
    let j = s
        .find(close)
        .unwrap_or_else(|| panic!("{close} 없음 ({entry})"))
        + close.len();
    s[i..j].to_string()
}

/// 재생성 경로의 최종 산출물을 만든다 (fixture → json → regen.hwpx).
fn regen() -> (PathBuf, PathBuf) {
    let json = tmp("fixture.json");
    let c = hwp()
        .arg("convert")
        .arg(fixture())
        .arg("-o")
        .arg(&json)
        .status()
        .unwrap();
    assert!(c.success(), "convert → json");
    let regen = tmp("regen.hwpx");
    let n = hwp()
        .arg("new")
        .arg("--from")
        .arg(&json)
        .arg("-o")
        .arg(&regen)
        .status()
        .unwrap();
    assert!(n.success(), "new --from json");
    (json, regen)
}

#[test]
fn regen_validate_and_cat_identical() {
    let (_, regen) = regen();
    let v = hwp().arg("validate").arg(&regen).output().unwrap();
    assert!(
        v.status.success(),
        "regen validate: {}",
        String::from_utf8_lossy(&v.stderr)
    );
    assert_eq!(
        cat(&fixture()),
        cat(&regen),
        "hwp cat stdout은 원본과 전문 동일해야"
    );
}

/// 표 하나의 지도 항목: (rows, cols, span 멀티셋, 셀 width 나열).
type TableMapEntry = (u16, u16, Vec<(u16, u16)>, Vec<i32>);

/// 표 지도: 재귀 순서로 TableMapEntry가 동일해야 한다.
#[test]
fn regen_table_map_identical() {
    let (_, regen) = regen();
    fn table_map(path: &Path) -> Vec<TableMapEntry> {
        let doc = hwpx::read_document(path).unwrap().document;
        fn walk<'a>(paras: &'a [hwp_model::Paragraph], out: &mut Vec<&'a hwp_model::Table>) {
            for p in paras {
                for c in &p.controls {
                    match c {
                        hwp_model::Control::Table(t) => {
                            out.push(t);
                            for cell in &t.cells {
                                walk(&cell.paragraphs, out);
                            }
                        }
                        hwp_model::Control::Generic(g) => {
                            for l in &g.paragraph_lists {
                                walk(&l.paragraphs, out);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        let mut tables = Vec::new();
        walk(&doc.sections[0].paragraphs, &mut tables);
        tables
            .iter()
            .map(|t| {
                let mut spans: Vec<(u16, u16)> =
                    t.cells.iter().map(|c| (c.col_span, c.row_span)).collect();
                spans.sort_unstable();
                let mut widths: Vec<i32> = t.cells.iter().map(|c| c.width.0).collect();
                widths.sort_unstable();
                (t.rows, t.cols, spans, widths)
            })
            .collect()
    }
    let src_map = table_map(&fixture());
    let regen_map = table_map(&regen);
    assert_eq!(src_map.len(), regen_map.len(), "표 개수 동일");
    assert_eq!(src_map, regen_map, "표 지도(rows/cols·span·width) 동일");
}

#[test]
fn regen_secpr_and_tabpr_byte_identical() {
    let (_, regen) = regen();
    // Gap B 게이트: secPr 슬라이스 바이트 동일.
    assert_eq!(
        zip_slice(
            &fixture(),
            "Contents/section0.xml",
            "<hp:secPr",
            "</hp:secPr>"
        ),
        zip_slice(&regen, "Contents/section0.xml", "<hp:secPr", "</hp:secPr>"),
        "secPr 바이트 동일"
    );
    // Gap C 게이트: tabProperties 슬라이스 바이트 동일.
    assert_eq!(
        zip_slice(
            &fixture(),
            "Contents/header.xml",
            "<hh:tabProperties",
            "</hh:tabProperties>"
        ),
        zip_slice(
            &regen,
            "Contents/header.xml",
            "<hh:tabProperties",
            "</hh:tabProperties>"
        ),
        "tabProperties 바이트 동일"
    );
}
