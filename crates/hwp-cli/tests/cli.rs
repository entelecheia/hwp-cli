//! `hwp` CLI 통합 테스트 — validate 종료코드 계약 (소비자가 exit code로 판정).

use std::path::PathBuf;
use std::process::Command;

fn hwp() -> Command {
    Command::new(env!("CARGO_BIN_EXE_hwp"))
}

fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

#[test]
fn validate_valid_hwpx_exit_zero() {
    let out = hwp()
        .arg("validate")
        .arg(fixture("hwpx/minimal.hwpx"))
        .output()
        .expect("hwp 실행");
    assert!(
        out.status.success(),
        "유효 hwpx는 exit 0 (stderr: {})",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn validate_corrupt_exit_nonzero_json() {
    let bad = std::env::temp_dir().join("hwp_cli_bad.hwpx");
    std::fs::write(&bad, b"this is not a valid hwp/hwpx file").unwrap();

    let out = hwp()
        .args(["validate", "--json"])
        .arg(&bad)
        .output()
        .expect("hwp 실행");
    assert!(!out.status.success(), "손상 파일은 비-0 종료");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"valid\": false") || stdout.contains("\"valid\":false"),
        "JSON에 valid:false (실제: {stdout})"
    );

    let _ = std::fs::remove_file(&bad);
}

#[test]
fn slots_json_shape() {
    // 합성 템플릿을 만들고 slots --json 구조 확인 (placeholders 배열).
    let tmp = std::env::temp_dir().join("hwp_cli_slots.hwpx");
    // hwp new로 {{name}}을 본문에 담은 hwpx 생성.
    let md = std::env::temp_dir().join("hwp_cli_slots.md");
    std::fs::write(&md, "{{기관명}} 본문 {{제목}}\n").unwrap();
    let mk = hwp()
        .args(["new", "--from"])
        .arg(&md)
        .arg("-o")
        .arg(&tmp)
        .output()
        .expect("hwp new");
    assert!(
        mk.status.success(),
        "hwp new: {}",
        String::from_utf8_lossy(&mk.stderr)
    );

    let out = hwp()
        .args(["slots", "--json"])
        .arg(&tmp)
        .output()
        .expect("hwp slots");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("placeholders"), "placeholders 키");
    assert!(
        stdout.contains("기관명") && stdout.contains("제목"),
        "자리표시자 이름"
    );

    let _ = std::fs::remove_file(&tmp);
    let _ = std::fs::remove_file(&md);
}
