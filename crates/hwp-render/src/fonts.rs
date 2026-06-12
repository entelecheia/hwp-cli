//! 폰트 해석 체인.
//!
//! 해석 순서: 문서의 FACE_NAME 이름 → 대체 글꼴 이름 → 한국어 폴백
//! 목록 → 임의의 시스템 글꼴. **조용한 대체 금지** — 모든 해석 결과를
//! 리포트에 남긴다(픽셀 정확도가 폰트에 좌우되므로).

use std::collections::HashMap;
use std::sync::Arc;

use fontdb::{Database, Family, Query, Source};
use hwp_model::Document;

/// 한국어 문서용 폴백 글꼴 (우선순위순).
const FALLBACKS: &[&str] = &[
    "함초롬바탕",
    "함초롬돋움",
    "Apple SD Gothic Neo",
    "AppleGothic",
    "NanumGothic",
    "나눔고딕",
    "Noto Sans CJK KR",
    "Noto Sans KR",
];

pub struct LoadedFont {
    pub data: Arc<Vec<u8>>,
    pub index: u32,
    /// 해석된 패밀리 이름 (리포트용)
    pub family: String,
}

pub struct FontStore {
    db: Database,
    /// fontdb ID → 로드된 폰트
    loaded: HashMap<fontdb::ID, Arc<LoadedFont>>,
    /// (요청 이름) → 해석 결과 캐시
    resolved: HashMap<String, Option<Arc<LoadedFont>>>,
    /// 해석 리포트 (요청 → 결과)
    pub report: Vec<String>,
}

impl FontStore {
    pub fn new() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        Self {
            db,
            loaded: HashMap::new(),
            resolved: HashMap::new(),
            report: Vec::new(),
        }
    }

    /// 추가 폰트 디렉터리 로드 (`--font-dir`).
    pub fn load_dir(&mut self, dir: &std::path::Path) {
        self.db.load_fonts_dir(dir);
    }

    /// 문서의 (언어 슬롯, 글꼴 ID)를 실제 폰트로 해석한다.
    pub fn resolve(
        &mut self,
        doc: &Document,
        lang_slot: usize,
        face_id: u16,
    ) -> Option<Arc<LoadedFont>> {
        let face = doc.header.fonts.get(lang_slot)?.get(face_id as usize);
        let requested = face.map(|f| f.name.clone()).unwrap_or_default();
        let alt = face.and_then(|f| f.alt_name.clone());

        if let Some(cached) = self.resolved.get(&requested) {
            return cached.clone();
        }

        let mut candidates: Vec<&str> = Vec::new();
        if !requested.is_empty() {
            candidates.push(&requested);
        }
        if let Some(alt) = &alt {
            candidates.push(alt);
        }
        candidates.extend(FALLBACKS);

        let mut result = None;
        for name in &candidates {
            if let Some(font) = self.try_family(name) {
                if *name != requested {
                    self.report
                        .push(format!("글꼴 대체: {requested:?} → {name:?}"));
                } else {
                    self.report.push(format!("글꼴 일치: {requested:?}"));
                }
                result = Some(font);
                break;
            }
        }
        // 최후 수단: 시스템 기본 산세리프 (CI 등 한국어 폰트 부재 환경)
        if result.is_none()
            && let Some(id) = self.db.query(&Query {
                families: &[Family::SansSerif],
                ..Query::default()
            })
            && let Some(font) = self.load_by_id(id)
        {
            self.report.push(format!(
                "글꼴 대체(최후): {requested:?} → 시스템 기본 {:?}",
                font.family
            ));
            result = Some(font);
        }
        if result.is_none() {
            self.report
                .push(format!("글꼴 해석 실패: {requested:?} (폴백 전부 없음)"));
        }
        self.resolved.insert(requested, result.clone());
        result
    }

    fn try_family(&mut self, name: &str) -> Option<Arc<LoadedFont>> {
        let id = self.db.query(&Query {
            families: &[Family::Name(name)],
            ..Query::default()
        })?;
        self.load_by_id(id)
    }

    fn load_by_id(&mut self, id: fontdb::ID) -> Option<Arc<LoadedFont>> {
        if let Some(loaded) = self.loaded.get(&id) {
            return Some(loaded.clone());
        }
        let face = self.db.face(id)?;
        let index = face.index;
        let family = face
            .families
            .first()
            .map(|(n, _)| n.clone())
            .unwrap_or_default();
        let data: Arc<Vec<u8>> = match &face.source {
            Source::File(path) => Arc::new(std::fs::read(path).ok()?),
            Source::Binary(bin) => Arc::new(bin.as_ref().as_ref().to_vec()),
            Source::SharedFile(_, bin) => Arc::new(bin.as_ref().as_ref().to_vec()),
        };
        let loaded = Arc::new(LoadedFont {
            data,
            index,
            family,
        });
        self.loaded.insert(id, loaded.clone());
        Some(loaded)
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new()
    }
}
