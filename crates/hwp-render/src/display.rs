//! DisplayList — 레이아웃과 백엔드 사이의 안정 계약.
//!
//! HWP 도메인 지식이 제거된 순수 그리기 명령. 좌표는 pt(f32),
//! 페이지 원점 좌상단, y축 아래 방향.

use std::sync::Arc;

use crate::shape::ShapedRun;

/// 합성 굵게(faux-bold) 스트로크 굵기 — 글자 크기 대비 비율.
/// 한컴 굵게 렌더와 대조해 보정한 값(글리프 윤곽선 위 중앙 스트로크).
pub const BOLD_STROKE: f32 = 0.045;

pub struct DisplayList {
    pub pages: Vec<PageList>,
}

pub struct PageList {
    pub width_pt: f32,
    pub height_pt: f32,
    pub items: Vec<Item>,
}

pub enum Item {
    /// 베이스라인 원점 (x, y)에 배치된 글리프 런
    Glyphs { x: f32, y: f32, run: ShapedRun },
    /// 채움 사각형 (셀 배경 등) — COLORREF
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: u32,
    },
    /// 선분 (테두리 등) — COLORREF, 굵기 pt
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: u32,
        width: f32,
    },
    /// 이미지 — 인코딩된 원본 바이트 (PNG/JPEG/BMP/GIF)
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        data: Arc<Vec<u8>>,
    },
}
