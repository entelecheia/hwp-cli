//! 최상위 문서 모델.

use serde::{Deserialize, Serialize};

use crate::control::{Control, SectionDef};
use crate::header::DocHeader;
use crate::paragraph::Paragraph;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// 출처 정보 (원본 포맷/버전 등)
    pub meta: DocMeta,
    pub header: DocHeader,
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocMeta {
    /// "hwp5" | "hwpx"
    pub source_format: String,
    /// 원본 파일 버전 (예: "5.1.0.1")
    pub source_version: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Section {
    pub paragraphs: Vec<Paragraph>,
    /// 문단이 아닌 최상위 레코드 (잘 형성된 파일에서는 비어 있음)
    pub extras: Vec<crate::opaque::OpaqueRecord>,
}

impl Section {
    /// 이 구역의 구역 정의 컨트롤 (보통 첫 문단의 첫 컨트롤).
    pub fn section_def(&self) -> Option<&SectionDef> {
        self.paragraphs
            .iter()
            .flat_map(|p| &p.controls)
            .find_map(|c| match c {
                Control::SectionDef(sd) => Some(sd),
                _ => None,
            })
    }
}
