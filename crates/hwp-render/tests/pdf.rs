//! PDF 백엔드 구조 + 서브셋 회귀 테스트.
//!
//! PDF 렌더러는 시각 검증이 불가하므로 (a) PDF 시그니처/페이지 구조, (b) 폰트가
//! 해석된 경우 임베드 구조(Type0/CIDFontType2/Identity-H/ToUnicode/FontFile2)와
//! 서브셋 크기 가드를 단언한다. 폰트 의존 단언은 폰트 가용성에 게이트한다
//! (render.rs 패턴 — 폰트 없는 CI/환경에서도 통과).

use hwp_render::{RenderOptions, render_document_pdf};

fn pdf_of(md: &str) -> Vec<u8> {
    let doc = hwp_convert::from_markdown(md);
    render_document_pdf(&doc, &RenderOptions::default()).bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[test]
fn pdf_signature_and_pages() {
    let bytes = pdf_of("# 제목\n\n본문 텍스트입니다.\n");
    assert!(bytes.starts_with(b"%PDF-"), "PDF 시그니처");
    assert!(contains(&bytes, b"/Pages"), "페이지 트리 존재");
    assert!(
        bytes.ends_with(b"%%EOF") || contains(&bytes, b"%%EOF"),
        "EOF 마커"
    );
}

#[test]
fn pdf_embedded_font_structure_when_font_available() {
    let bytes = pdf_of("# 제목\n\n본문 텍스트입니다. Hello world.\n");
    // 폰트가 해석돼 글리프가 임베드된 경우에만 구조/서브셋 단언.
    if contains(&bytes, b"/FontFile2") {
        assert!(contains(&bytes, b"Type0"), "Type0 합성 폰트");
        assert!(contains(&bytes, b"CIDFontType2"), "CIDFontType2 자손 폰트");
        assert!(contains(&bytes, b"Identity-H"), "Identity-H 인코딩");
        assert!(contains(&bytes, b"/ToUnicode"), "ToUnicode 참조");
        assert!(contains(&bytes, b"begincmap"), "ToUnicode CMap 스트림");
        // 서브셋 동작: 짧은 문서는 전체 폰트(수 MB)가 아니라 작아야 한다.
        assert!(
            bytes.len() < 2_000_000,
            "서브셋 PDF는 2MB 미만이어야 함 (실제 {}B) — 서브셋 회귀 가능성",
            bytes.len()
        );
    }
}

#[test]
fn pdf_empty_document() {
    let bytes = pdf_of("");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(contains(&bytes, b"/Pages"));
}
