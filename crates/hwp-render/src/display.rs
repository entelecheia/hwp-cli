//! DisplayList — 레이아웃과 백엔드 사이의 안정 계약.
//!
//! HWP 도메인 지식이 제거된 순수 그리기 명령. 좌표는 pt(f32),
//! 페이지 원점 좌상단, y축 아래 방향.

use crate::shape::ShapedRun;

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
}
