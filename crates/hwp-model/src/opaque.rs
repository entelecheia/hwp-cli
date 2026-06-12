//! 알 수 없는 데이터의 무손실 보존.
//!
//! 무손실 전략의 핵심: 모르는 레코드는 버리지 않고 원시 형태로 운반한다.
//! 같은 포맷 재저장 시 그대로 방출하고, 교차 포맷 변환 시에는 드롭하되
//! 경고로 집계한다.

use serde::{Deserialize, Serialize};

/// 해석하지 못한 HWP 5.0 레코드 (서브트리 통째).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueRecord {
    pub tag: u16,
    #[serde(with = "hex_bytes")]
    pub data: Vec<u8>,
    pub children: Vec<OpaqueRecord>,
}

/// serde에서 바이트 열을 hex 문자열로 직렬화 (스냅샷 가독성).
pub mod hex_bytes {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        s.serialize_str(&hex)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        if !s.len().is_multiple_of(2) {
            return Err(serde::de::Error::custom(
                "hex 문자열 길이는 짝수여야 합니다",
            ));
        }
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}
