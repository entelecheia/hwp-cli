//! 참조 테이블 ID newtype들.
//!
//! DocHeader의 각 테이블에 대한 인덱스. 종류가 다른 ID를 섞어 쓰는
//! 실수를 타입으로 방지한다.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$doc:meta] $name:ident),* $(,)?) => {
        $(
            #[$doc]
            #[derive(
                Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash,
                Serialize, Deserialize,
            )]
            #[serde(transparent)]
            pub struct $name(pub u16);
        )*
    };
}

id_type! {
    /// DocHeader::char_shapes 인덱스
    CharShapeId,
    /// DocHeader::para_shapes 인덱스
    ParaShapeId,
    /// DocHeader::styles 인덱스
    StyleId,
    /// DocHeader::border_fills 참조 — 주의: HWP에서 1-기반 관례
    BorderFillId,
    /// 언어 슬롯별 DocHeader::fonts 인덱스
    FaceNameId,
    /// BinData 항목 참조
    BinDataId,
}
