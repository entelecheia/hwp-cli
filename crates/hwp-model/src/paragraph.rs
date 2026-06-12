//! 문단과 문자 모델.
//!
//! HWP 텍스트는 UTF-16 코드 유닛(WCHAR) 열이며 0~31은 컨트롤 문자다.
//! 컨트롤 문자 분류표([`char_kind`])는 reader/writer/텍스트 추출 모두의
//! **단일 진실 공급원**이다 — 확장/인라인 컨트롤(8 WCHAR)을 잘못 세면
//! 이후 모든 위치 계산이 어긋난다.

use serde::{Deserialize, Serialize};

use crate::control::Control;
use crate::ids::{CharShapeId, ParaShapeId, StyleId};
use crate::opaque::{OpaqueRecord, hex_bytes};

/// 컨트롤 문자 분류 (한글문서파일형식 5.0 §4.2.4 표).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CharKind {
    /// 1 WCHAR — 그 자체로 의미를 갖는 문자형 컨트롤 (0, 10, 13, 24~31)
    Char,
    /// 8 WCHAR — [코드, 정보 6, 코드] 인라인 컨트롤 (4~9, 19, 20)
    Inline,
    /// 8 WCHAR — 별도 CTRL_HEADER 레코드를 가리키는 확장 컨트롤
    /// (1~3, 11, 12, 14~18, 21~23)
    Extended,
}

/// 컨트롤 문자 분류표 — 단일 진실 공급원.
pub fn char_kind(code: u16) -> CharKind {
    match code {
        0 | 10 | 13 | 24..=31 => CharKind::Char,
        4..=9 | 19 | 20 => CharKind::Inline,
        1..=3 | 11 | 12 | 14..=18 | 21..=23 => CharKind::Extended,
        _ => CharKind::Char, // 32 이상 = 일반 문자
    }
}

/// 잘 알려진 컨트롤 문자 코드.
pub mod ctrl_char {
    pub const LINE_BREAK: u16 = 10;
    pub const PARA_BREAK: u16 = 13;
    pub const HYPHEN: u16 = 24;
    pub const NB_SPACE: u16 = 30; // 묶음 빈칸
    pub const FW_SPACE: u16 = 31; // 고정폭 빈칸
    pub const FIELD_END: u16 = 4;
    pub const TAB: u16 = 9;
    pub const SECTION_COLUMN_DEF: u16 = 2; // 구역/단 정의
    pub const FIELD_START: u16 = 3;
    pub const OBJECT: u16 = 11; // 그리기 개체/표
    pub const HIDDEN_COMMENT: u16 = 15;
    pub const HEADER_FOOTER: u16 = 16;
    pub const FOOTNOTE_ENDNOTE: u16 = 17;
    pub const AUTO_NUMBER: u16 = 18;
    pub const PAGE_CONTROL: u16 = 21;
    pub const BOOKMARK: u16 = 22;
    pub const OVERLAP: u16 = 23; // 덧말/글자 겹침
}

/// 문단을 구성하는 문자 하나.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HwpChar {
    /// 일반 문자 (서로게이트 쌍은 합쳐서 char 하나)
    Text(char),
    /// 1 WCHAR 문자형 컨트롤
    CharCtrl(u16),
    /// 8 WCHAR 인라인 컨트롤 — payload는 정보 6 WCHAR(12바이트)
    InlineCtrl {
        code: u16,
        #[serde(with = "hex_bytes")]
        payload: Vec<u8>,
    },
    /// 8 WCHAR 확장 컨트롤 — `Paragraph::controls[ctrl_index]`를 가리킴
    ExtCtrl {
        code: u16,
        /// 정방향 ctrl_id (예: b"secd") — 스트림에는 역순으로 저장됨
        ctrl_id: [u8; 4],
        /// 정보 6 WCHAR 원본 12바이트 (선두 4바이트 = 역순 ctrl_id) — 왕복 보존용
        #[serde(with = "hex_bytes")]
        payload: Vec<u8>,
        /// 대응하는 컨트롤 인덱스 (CTRL_HEADER 매칭 실패 시 None)
        ctrl_index: Option<u32>,
    },
}

impl HwpChar {
    /// 이 문자가 차지하는 WCHAR 수. 위치 계산의 기준.
    pub fn wchar_width(&self) -> u32 {
        match self {
            HwpChar::Text(c) => c.len_utf16() as u32,
            HwpChar::CharCtrl(_) => 1,
            HwpChar::InlineCtrl { .. } | HwpChar::ExtCtrl { .. } => 8,
        }
    }
}

/// PARA_LINE_SEG의 줄 하나 (36바이트).
/// 한글이 저장한 줄 배치 — 렌더러가 그대로 신뢰하는 1급 입력.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineSeg {
    /// 줄 시작 텍스트 위치 (문단 내 WCHAR 오프셋)
    pub text_start: u32,
    /// 줄 세로 위치
    pub v_pos: i32,
    /// 줄 높이
    pub line_height: i32,
    /// 텍스트 부분 높이
    pub text_height: i32,
    /// 줄 세로 위치에서 베이스라인까지 거리
    pub baseline_gap: i32,
    /// 줄간격
    pub line_spacing: i32,
    /// 컬럼에서의 시작 위치
    pub col_start: i32,
    /// 세그먼트 폭
    pub seg_width: i32,
    /// 플래그 (페이지 첫 줄, 컬럼 첫 줄, 빈 세그먼트 등)
    pub flags: u32,
}

/// PARA_HEADER에서 보존하는 부가 정보.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParaHeaderInfo {
    /// nchars 필드의 최상위 비트 등 플래그
    pub chars_flags: u8,
    pub ctrl_mask: u32,
    /// 단 나누기 종류 비트
    pub break_type: u8,
    pub instance_id: u32,
    /// 버전에 따라 붙는 꼬리 (변경 추적 병합 등) — 왕복 보존
    #[serde(with = "hex_bytes")]
    pub tail: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Paragraph {
    pub para_shape: ParaShapeId,
    pub style: StyleId,
    pub chars: Vec<HwpChar>,
    /// (WCHAR 시작 위치, 문자 모양 ID) — PARA_CHAR_SHAPE
    pub char_shape_runs: Vec<(u32, CharShapeId)>,
    /// PARA_LINE_SEG — 비어 있으면 줄 배치 정보 없음 (렌더러 폴백 경로)
    pub line_segs: Vec<LineSeg>,
    /// 확장 컨트롤이 가리키는 컨트롤들 (등장 순서)
    pub controls: Vec<Control>,
    pub header: ParaHeaderInfo,
    /// 해석하지 못한 자식 레코드
    pub extras: Vec<OpaqueRecord>,
}

impl Paragraph {
    /// 총 WCHAR 수 (PARA_HEADER nchars와 대조용).
    pub fn wchar_len(&self) -> u32 {
        self.chars.iter().map(HwpChar::wchar_width).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 분류표가 0~31 전 영역을 빠짐없이 커버하는지, 스펙 표와 일치하는지.
    #[test]
    fn 컨트롤_문자_분류표() {
        let char_codes = [0u16, 10, 13, 24, 25, 26, 27, 28, 29, 30, 31];
        let inline_codes = [4u16, 5, 6, 7, 8, 9, 19, 20];
        let ext_codes = [1u16, 2, 3, 11, 12, 14, 15, 16, 17, 18, 21, 22, 23];

        assert_eq!(char_codes.len() + inline_codes.len() + ext_codes.len(), 32);
        for c in char_codes {
            assert_eq!(char_kind(c), CharKind::Char, "code {c}");
        }
        for c in inline_codes {
            assert_eq!(char_kind(c), CharKind::Inline, "code {c}");
        }
        for c in ext_codes {
            assert_eq!(char_kind(c), CharKind::Extended, "code {c}");
        }
        // 일반 문자
        assert_eq!(char_kind(32), CharKind::Char);
        assert_eq!(char_kind(0xAC00), CharKind::Char); // '가'
    }

    #[test]
    fn wchar_너비() {
        assert_eq!(HwpChar::Text('가').wchar_width(), 1);
        assert_eq!(HwpChar::Text('𝕏').wchar_width(), 2); // 서로게이트 쌍
        assert_eq!(HwpChar::CharCtrl(13).wchar_width(), 1);
        let ext = HwpChar::ExtCtrl {
            code: 2,
            ctrl_id: *b"secd",
            payload: vec![0; 12],
            ctrl_index: None,
        };
        assert_eq!(ext.wchar_width(), 8);
    }
}
