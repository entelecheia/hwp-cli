//! IR → PNG/SVG/PDF 페이지 렌더러.
//!
//! 파이프라인: IR → Layout([`layout`] — LineSegLayouter) →
//! [`display::DisplayList`] → 백엔드([`png`] — tiny-skia).
//! SVG(M5)/PDF(M7) 백엔드는 이후 마일스톤에서 추가한다.
//!
//! v1 범위: lineseg 기반 텍스트 렌더링 (굵게/기울임/크기/색/자간/장평,
//! 가운데/오른쪽 정렬). 표·이미지·장식은 M5.

pub mod display;
pub mod error;
pub mod fonts;
pub mod layout;
pub mod png;
pub mod shape;

use hwp_model::Document;

pub use error::RenderError;
pub use fonts::FontStore;

pub struct RenderOptions {
    pub dpi: f32,
    /// 추가 폰트 디렉터리
    pub font_dirs: Vec<std::path::PathBuf>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            dpi: 96.0,
            font_dirs: Vec::new(),
        }
    }
}

pub struct RenderOutput {
    /// 페이지별 래스터 (PNG 인코딩 전)
    pub pages: Vec<tiny_skia::Pixmap>,
    /// 경고 + 폰트 해석 리포트
    pub report: Vec<String>,
}

/// 문서 전체를 렌더링한다.
pub fn render_document(doc: &Document, opts: &RenderOptions) -> Result<RenderOutput, RenderError> {
    let mut store = FontStore::new();
    for dir in &opts.font_dirs {
        store.load_dir(dir);
    }
    let mut warnings = Vec::new();
    let list = layout::layout_document(doc, &mut store, &mut warnings);
    let pages = png::render_png(&list, opts.dpi)?;
    warnings.append(&mut store.report);
    Ok(RenderOutput {
        pages,
        report: warnings,
    })
}
