//! LineSegLayouter — 파일에 저장된 줄 배치(PARA_LINE_SEG)를 복원해
//! DisplayList를 만든다.
//!
//! 실측으로 확정한 좌표 해석 (U1):
//! - `v_pos`: 페이지 본문 영역 상단 기준, 페이지마다 0으로 리셋
//! - 베이스라인 y = body_top + v_pos + baseline_gap
//! - `col_start`/`seg_width`: 본문 영역 왼쪽 기준
//! - 페이지 경계: v_pos가 직전 줄보다 작아지면 새 페이지 (v1 휴리스틱)
//!
//! lineseg가 없는 문단(프로그램 생성 hwpx 등)은 단순 폴백:
//! 직전 줄 아래에 한 줄로 배치 (줄바꿈 없음 — FlowLayouter는 M7).

use hwp_model::{Document, HwpUnit, PageDef};

use crate::display::{DisplayList, Item, PageList};
use crate::fonts::FontStore;
use crate::shape::{InlineItem, shape_range};

/// 기본 탭 간격 (40pt = 4000 HWPUNIT).
const TAB_INTERVAL_PT: f32 = 40.0;

/// A4 기본값 (PAGE_DEF가 없는 비정상 문서 방어).
fn default_page() -> PageDef {
    PageDef {
        width: HwpUnit(59528),
        height: HwpUnit(84186),
        margin_left: HwpUnit(8504),
        margin_right: HwpUnit(8504),
        margin_top: HwpUnit(5668),
        margin_bottom: HwpUnit(4252),
        margin_header: HwpUnit(4252),
        margin_footer: HwpUnit(4252),
        gutter: HwpUnit(0),
        attr: 0,
    }
}

pub fn layout_document(
    doc: &Document,
    store: &mut FontStore,
    warnings: &mut Vec<String>,
) -> DisplayList {
    let mut pages = Vec::new();

    for section in &doc.sections {
        let page_def = section
            .section_def()
            .and_then(|d| d.page)
            .unwrap_or_else(|| {
                warnings.push("PAGE_DEF 없음 — A4 기본값 사용".to_string());
                default_page()
            });
        let (w, h) = (
            page_def.width.to_pt() as f32,
            page_def.height.to_pt() as f32,
        );
        let body_left = page_def.margin_left.to_pt() as f32;
        let body_top = (page_def.margin_top.0 + page_def.margin_header.0) as f32 / 100.0;
        let body_width =
            (page_def.width.0 - page_def.margin_left.0 - page_def.margin_right.0) as f32 / 100.0;

        let mut page = PageList {
            width_pt: w,
            height_pt: h,
            items: Vec::new(),
        };
        let mut prev_v_pos = -1i32;
        let mut fallback_y_pt = 0.0f32; // lineseg 없는 문단용 누적 y (body 기준)
        let mut skipped_controls = 0usize;

        for para in &section.paragraphs {
            skipped_controls += para
                .controls
                .iter()
                .filter(|c| !matches!(c, hwp_model::Control::SectionDef(_)))
                .filter(|c| c.ctrl_id() != *b"cold")
                .count();

            if para.line_segs.is_empty() {
                // 폴백: 한 줄 배치 (줄바꿈 없음)
                if para.chars.is_empty() {
                    fallback_y_pt += 16.0; // 빈 문단 높이 근사
                    continue;
                }
                let end = para.wchar_len();
                let items = shape_range(store, doc, para, (0, end), warnings);
                let max_size = items_max_size(&items).unwrap_or(10.0);
                fallback_y_pt += max_size * 1.6;
                place_line(&mut page, items, body_left, body_top + fallback_y_pt);
                continue;
            }

            for (i, seg) in para.line_segs.iter().enumerate() {
                // 페이지 경계: v_pos 리셋 감지
                if seg.v_pos < prev_v_pos && !page.items.is_empty() {
                    pages.push(std::mem::replace(
                        &mut page,
                        PageList {
                            width_pt: w,
                            height_pt: h,
                            items: Vec::new(),
                        },
                    ));
                }
                prev_v_pos = seg.v_pos;

                let line_start = seg.text_start;
                let line_end = para
                    .line_segs
                    .get(i + 1)
                    .map_or(para.wchar_len(), |next| next.text_start);
                if line_end <= line_start {
                    continue;
                }

                let items = shape_range(store, doc, para, (line_start, line_end), warnings);
                let natural_width: f32 = items_width(&items);

                // 정렬 보정 (가운데/오른쪽만 — 양쪽 정렬 잉여 분배는 U2)
                let seg_width_pt = seg.seg_width as f32 / 100.0;
                let align = doc
                    .header
                    .para_shapes
                    .get(para.para_shape.0 as usize)
                    .map_or(0, |ps| ps.alignment());
                let shift = match align {
                    2 => (seg_width_pt - natural_width).max(0.0), // 오른쪽
                    3 => ((seg_width_pt - natural_width) / 2.0).max(0.0), // 가운데
                    _ => 0.0,
                };

                let x = body_left + seg.col_start as f32 / 100.0 + shift;
                let baseline_y = body_top + (seg.v_pos + seg.baseline_gap) as f32 / 100.0;
                place_line(&mut page, items, x, baseline_y);
                fallback_y_pt = (seg.v_pos + seg.line_height) as f32 / 100.0;
            }
        }
        let _ = body_width; // (양쪽 정렬 분배 시 사용 예정)
        if skipped_controls > 0 {
            warnings.push(format!(
                "렌더 미지원 컨트롤 {skipped_controls}개 생략 (표/개체 — M5 예정)"
            ));
        }
        pages.push(page);
    }

    DisplayList { pages }
}

fn items_width(items: &[InlineItem]) -> f32 {
    let mut x = 0.0f32;
    for item in items {
        match item {
            InlineItem::Run(run) => x += run.width_pt,
            InlineItem::Tab => {
                x = (x / TAB_INTERVAL_PT).floor() * TAB_INTERVAL_PT + TAB_INTERVAL_PT
            }
        }
    }
    x
}

fn items_max_size(items: &[InlineItem]) -> Option<f32> {
    items
        .iter()
        .filter_map(|i| match i {
            InlineItem::Run(r) => Some(r.size_pt),
            InlineItem::Tab => None,
        })
        .reduce(f32::max)
}

fn place_line(page: &mut PageList, items: Vec<InlineItem>, x0: f32, baseline_y: f32) {
    let mut x = x0;
    for item in items {
        match item {
            InlineItem::Run(run) => {
                let w = run.width_pt;
                page.items.push(Item::Glyphs {
                    x,
                    y: baseline_y,
                    run,
                });
                x += w;
            }
            InlineItem::Tab => {
                let rel = x - x0;
                x = x0 + (rel / TAB_INTERVAL_PT).floor() * TAB_INTERVAL_PT + TAB_INTERVAL_PT;
            }
        }
    }
}
