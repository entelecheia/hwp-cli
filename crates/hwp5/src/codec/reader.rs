//! 리틀엔디언 바이트 커서.
//!
//! 모든 읽기는 실패 시 `Err`를 반환한다 — panic 금지 (손상 파일 내성).

use crate::error::{Hwp5Error, Result};

pub struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }

    /// 남은 바이트 전부를 복사 없이 반환하고 커서를 끝으로 옮긴다.
    pub fn take_rest(&mut self) -> &'a [u8] {
        let rest = &self.data[self.pos..];
        self.pos = self.data.len();
        rest
    }

    pub fn read_bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.remaining() < n {
            return Err(Hwp5Error::UnexpectedEof {
                offset: self.pos,
                wanted: n,
                remaining: self.remaining(),
            });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn read_u8(&mut self) -> Result<u8> {
        Ok(self.read_bytes(1)?[0])
    }

    pub fn read_u16(&mut self) -> Result<u16> {
        let b = self.read_bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn read_u32(&mut self) -> Result<u32> {
        let b = self.read_bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_i32(&mut self) -> Result<i32> {
        Ok(self.read_u32()? as i32)
    }

    /// WCHAR(UTF-16LE 코드 유닛) n개를 읽는다.
    pub fn read_wchars(&mut self, n: usize) -> Result<Vec<u16>> {
        let b = self.read_bytes(n * 2)?;
        Ok(b.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect())
    }

    pub fn read_i8(&mut self) -> Result<i8> {
        Ok(self.read_u8()? as i8)
    }

    /// HWP 문자열: WORD 길이 + UTF-16LE 문자들.
    pub fn read_hwp_string(&mut self) -> Result<String> {
        let len = self.read_u16()? as usize;
        let units = self.read_wchars(len)?;
        Ok(String::from_utf16_lossy(&units))
    }

    /// u16 배열을 고정 크기로 읽는다.
    pub fn read_u16_array<const N: usize>(&mut self) -> Result<[u16; N]> {
        let mut out = [0u16; N];
        for slot in &mut out {
            *slot = self.read_u16()?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 기본_읽기() {
        let mut r = ByteReader::new(&[0x01, 0x02, 0x03, 0x04, 0x05]);
        assert_eq!(r.read_u8().unwrap(), 0x01);
        assert_eq!(r.read_u16().unwrap(), 0x0302);
        assert_eq!(r.remaining(), 2);
        assert!(r.read_u32().is_err()); // 부족하면 Err, panic 아님
    }
}
