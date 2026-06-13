//! 합성 문서(md/hwpx 출신)의 줄 배치(PARA_LINE_SEG) 합성.
//!
//! 5.1.x 한글은 본문 문단에 줄 배치 캐시가 없으면 글자를 0 높이로 그려
//! '검은 바'/'빈 내용'/'손상'으로 표시한다. 단순 합성(문단당 1줄)은 여러 줄
//! 문단·긴 표 셀에서 겹침/높이 붕괴를 일으킨다. 여기서는 폰트 셰이핑
//! (shape_range)으로 글자 폭을 측정해 본문 폭 기준 그리디 줄바꿈을 하고,
//! 줄 수만큼 PARA_LINE_SEG를 생성한다. v_pos는 섹션/셀 내 누적.
//!
//! 정확도의 핵심은 한글과 동일한 폰트(함초롬바탕)로 셰이핑하는 것이다.

use hwp_model::{Control, Document, LineSeg, Paragraph};

use crate::fonts::FontStore;
use crate::shape::{InlineItem, shape_range};

/// 탭 간격 (pt) — layout.rs place_wrapped와 동일해야 한다.
const TAB_INTERVAL_PT: f32 = 40.0;

/// 합성 문서 전체에 줄 배치를 합성한다 (본문·표 셀 문단).
/// `store`는 함초롬바탕이 로드된 FontStore여야 한글과 줄바꿈이 일치한다.
pub fn synthesize_linesegs(doc: &mut Document, store: &mut FontStore, warnings: &mut Vec<String>) {
    let snap = doc.clone();
    for si in 0..doc.sections.len() {
        let body_width = snap.sections[si]
            .section_def()
            .and_then(|d| d.page.as_ref())
            .map_or(42520, |pg| pg.width.0 - pg.margin_left.0 - pg.margin_right.0);
        let mut v_pos = 0i32;
        for pi in 0..doc.sections[si].paragraphs.len() {
            let src = &snap.sections[si].paragraphs[pi];
            let segs = compute_linesegs(store, &snap, src, body_width, &mut v_pos, warnings);
            doc.sections[si].paragraphs[pi].line_segs = segs;
            fill_nested(si, pi, &snap, doc, store, warnings);
        }
    }
}

/// 표 셀 안 문단에도 줄 배치를 합성한다 (셀 폭 기준, 셀마다 v_pos 리셋).
fn fill_nested(
    si: usize,
    pi: usize,
    snap: &Document,
    doc: &mut Document,
    store: &mut FontStore,
    warnings: &mut Vec<String>,
) {
    let nctrl = doc.sections[si].paragraphs[pi].controls.len();
    for ci in 0..nctrl {
        let Control::Table(snap_t) = &snap.sections[si].paragraphs[pi].controls[ci] else {
            continue;
        };
        // 셀별 (본문 폭, 문단 수)을 먼저 수집 (snap 불변 참조).
        let cells: Vec<(i32, usize)> = snap_t
            .cells
            .iter()
            .map(|c| {
                let w =
                    (c.width.0 - i32::from(c.margins[0]) - i32::from(c.margins[1])).max(1);
                (w, c.paragraphs.len())
            })
            .collect();
        for (celli, &(cw, npara)) in cells.iter().enumerate() {
            let mut cv = 0i32;
            for cpi in 0..npara {
                let Control::Table(snap_t) = &snap.sections[si].paragraphs[pi].controls[ci] else {
                    unreachable!();
                };
                let csrc = &snap_t.cells[celli].paragraphs[cpi];
                let segs = compute_linesegs(store, snap, csrc, cw, &mut cv, warnings);
                if let Control::Table(t) = &mut doc.sections[si].paragraphs[pi].controls[ci] {
                    t.cells[celli].paragraphs[cpi].line_segs = segs;
                }
            }
        }
    }
}

/// 한 문단의 줄 배치를 계산한다. `v_pos`는 섹션/셀 내 세로 누적 커서.
/// 빈 문단도 줄 배치 1개를 가진다(한글 본문 표시 전제).
fn compute_linesegs(
    store: &mut FontStore,
    doc: &Document,
    para: &Paragraph,
    body_width: i32,
    v_pos: &mut i32,
    warnings: &mut Vec<String>,
) -> Vec<LineSeg> {
    // 줄 높이/간격은 문단 첫 글자 모양의 기준 크기에서 유도(정품 가나다 실측:
    // line_height=base, baseline_gap=base*0.85, line_spacing=base*0.6=줄간격 160%).
    let base = para
        .char_shape_runs
        .first()
        .and_then(|(_, id)| doc.header.char_shapes.get(id.0 as usize))
        .map_or(1000, |cs| if cs.base_size > 0 { cs.base_size } else { 1000 });
    let line_spacing = base * 60 / 100;
    let line_advance = base + line_spacing;
    let baseline_gap = base * 85 / 100;
    let seg_width = body_width.max(1);
    let limit_pt = seg_width as f32 / 100.0;
    let total = para.wchar_len();

    let make = |start: u32, v: i32| LineSeg {
        text_start: start,
        v_pos: v,
        line_height: base,
        text_height: base,
        baseline_gap,
        line_spacing,
        col_start: 0,
        seg_width,
        flags: 0x0006_0000,
    };

    // 폰트 셰이핑으로 글자 폭을 재고, 본문 폭 기준 그리디 줄바꿈.
    // place_wrapped(layout.rs)와 동일한 글리프 x_advance 누적 규칙.
    let items = shape_range(store, doc, para, (0, total), warnings);
    let mut segs = Vec::new();
    let mut line_start = 0u32;
    let mut acc = 0.0f32;
    let mut content = false;
    for item in &items {
        match item {
            InlineItem::Run(run) => {
                for (gi, g) in run.glyphs.iter().enumerate() {
                    if content && acc + g.x_advance > limit_pt {
                        segs.push(make(line_start, *v_pos));
                        *v_pos += line_advance;
                        line_start = run.start_wchar + gi as u32;
                        acc = 0.0;
                    }
                    acc += g.x_advance;
                    content = true;
                }
            }
            InlineItem::Tab => {
                acc = (acc / TAB_INTERVAL_PT).floor() * TAB_INTERVAL_PT + TAB_INTERVAL_PT;
                content = true;
            }
        }
    }
    // 마지막 줄(빈 문단이면 유일한 줄).
    segs.push(make(line_start, *v_pos));
    *v_pos += line_advance;
    segs
}
