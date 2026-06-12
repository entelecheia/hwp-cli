//! LineSegLayouter — 파일에 저장된 줄 배치(PARA_LINE_SEG)를 복원해
//! DisplayList를 만든다.
//!
//! 실측으로 확정한 좌표 해석 (U1):
//! - `v_pos`: 페이지 본문 영역 상단 기준, 페이지마다 0으로 리셋
//! - 베이스라인 y = body_top + v_pos + baseline_gap
//! - `col_start`/`seg_width`: 본문 영역 왼쪽 기준
//! - 페이지 경계: v_pos가 직전 줄보다 작아지면 새 페이지 (v1 휴리스틱)
//!
//! 불완전한 파일 대응 (실무 hwpx에서 실측):
//! - 도구 생성 파일은 문단당 lineseg 1개 + 문단당 1줄 가정의 v_pos를
//!   기록한다 → seg 폭에서 그리디 줄바꿈 + **흐름 커서**로 보정한다.
//!   베이스라인 = max(저장된 v_pos 기반, 직전 콘텐츠 하단 기반) —
//!   완전한 파일에서는 저장값이 항상 크므로 무손실, 불완전 파일에서는
//!   겹침만 아래로 밀어낸다.
//! - lineseg가 아예 없는 문단은 본문 폭 기준 폴백 배치.

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
        // 흐름 커서: 이 페이지에 실제 배치된 콘텐츠의 하단 y (page 좌표)
        let mut content_bottom = body_top;
        let mut skipped_controls = 0usize;

        for para in &section.paragraphs {
            skipped_controls += para
                .controls
                .iter()
                .filter(|c| !matches!(c, hwp_model::Control::SectionDef(_)))
                .filter(|c| c.ctrl_id() != *b"cold")
                .count();

            if para.line_segs.is_empty() {
                // 폴백: 본문 폭에서 그리디 줄바꿈
                if para.chars.is_empty() {
                    content_bottom += 16.0; // 빈 문단 높이 근사
                    continue;
                }
                let end = para.wchar_len();
                let items = shape_range(store, doc, para, (0, end), warnings);
                let max_size = items_max_size(&items).unwrap_or(10.0);
                let baseline_y = content_bottom + max_size * 1.2;
                let last_y = place_wrapped(
                    &mut page,
                    items,
                    body_left,
                    baseline_y,
                    body_width,
                    max_size * 1.6,
                );
                content_bottom = last_y + max_size * 0.4;
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
                    content_bottom = body_top;
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

                let baseline_gap_pt = seg.baseline_gap as f32 / 100.0;
                let line_height_pt = seg.line_height as f32 / 100.0;
                let stored_baseline = body_top + (seg.v_pos + seg.baseline_gap) as f32 / 100.0;
                // 흐름 커서 보정: 앞 콘텐츠가 저장 위치를 이미 지났으면
                // 베이스라인을 (콘텐츠 하단 + 이 줄의 ascent) 아래로 밀어낸다
                let baseline_y = stored_baseline.max(content_bottom + baseline_gap_pt);

                // 문단에 lineseg가 1개뿐인데 텍스트가 폭을 넘으면 불완전한
                // lineseg로 보고 seg 폭에서 줄바꿈. 완전한 lineseg는 신뢰.
                let wrap_width = if para.line_segs.len() == 1 {
                    seg_width_pt.max(10.0)
                } else {
                    f32::INFINITY
                };
                let line_advance =
                    (seg.line_height + seg.line_spacing).max(seg.line_height) as f32 / 100.0;

                let x = body_left + seg.col_start as f32 / 100.0 + shift;
                let last_y =
                    place_wrapped(&mut page, items, x, baseline_y, wrap_width, line_advance);
                content_bottom = last_y + (line_height_pt - baseline_gap_pt).max(0.0);
            }
        }
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

/// 인라인 항목들을 배치한다. `max_width`를 넘으면 글리프 단위 그리디
/// 줄바꿈(`f32::INFINITY`면 비활성). 마지막 베이스라인 y를 반환한다.
fn place_wrapped(
    page: &mut PageList,
    items: Vec<InlineItem>,
    x0: f32,
    first_baseline_y: f32,
    max_width: f32,
    line_advance: f32,
) -> f32 {
    let limit = x0 + max_width;
    let mut x = x0;
    let mut y = first_baseline_y;

    if std::env::var_os("HWP_RENDER_TRACE").is_some() {
        let preview: String = items
            .iter()
            .filter_map(|i| match i {
                InlineItem::Run(r) => Some(r.text.as_str()),
                InlineItem::Tab => None,
            })
            .collect::<String>()
            .chars()
            .take(20)
            .collect();
        eprintln!("TRACE y={first_baseline_y:.1} x={x0:.1} wrap={max_width:.0} [{preview}]");
    }

    for item in items {
        match item {
            InlineItem::Run(run) => {
                if max_width.is_infinite() || x + run.width_pt <= limit {
                    let w = run.width_pt;
                    page.items.push(Item::Glyphs { x, y, run });
                    x += w;
                    continue;
                }
                // 글리프 단위 분할 (CJK는 글자 사이 어디서나 분리 가능)
                let mut start = 0usize;
                let mut piece_x = x;
                let mut acc = 0.0f32;
                for (i, g) in run.glyphs.iter().enumerate() {
                    let over = piece_x + acc + g.x_advance > limit;
                    let line_has_content = i > start || piece_x > x0;
                    if over && line_has_content {
                        if i > start {
                            let piece = run.slice(start, i);
                            page.items.push(Item::Glyphs {
                                x: piece_x,
                                y,
                                run: piece,
                            });
                        }
                        y += line_advance;
                        piece_x = x0;
                        acc = 0.0;
                        start = i;
                    }
                    acc += g.x_advance;
                }
                if start < run.glyphs.len() {
                    let piece = run.slice(start, run.glyphs.len());
                    let w = piece.width_pt;
                    page.items.push(Item::Glyphs {
                        x: piece_x,
                        y,
                        run: piece,
                    });
                    x = piece_x + w;
                } else {
                    x = piece_x;
                }
            }
            InlineItem::Tab => {
                let rel = x - x0;
                x = x0 + (rel / TAB_INTERVAL_PT).floor() * TAB_INTERVAL_PT + TAB_INTERVAL_PT;
            }
        }
    }
    y
}
