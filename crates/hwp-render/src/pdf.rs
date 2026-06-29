//! PDF 백엔드 — DisplayList를 텍스트 선택가능 PDF로 직렬화.
//!
//! 글리프는 임베드된 CIDFontType2(Identity-H) + ToUnicode CMap으로 그린다 →
//! 화면 표시와 텍스트 선택/검색 모두 가능. 위치는 DisplayList의 펜 좌표를 글리프마다
//! 텍스트 행렬(Tm)로 직접 지정하므로 SVG/PNG 백엔드와 동일 레이아웃을 보장한다
//! (폰트 advance 메트릭 비의존). 좌표 변환: PDF는 좌하단 원점·y 상향 → `pdf_y =
//! page_height - hwp_y`.
//!
//! 폰트는 사용 글리프만 서브셋 임베드. 이미지(Item::Image)는 PDF Image XObject로
//! 임베드한다 — JPEG는 DCTDecode 무손실 통과, PNG/BMP/GIF는 디코드 후 FlateDecode RGB
//! (알파는 흰색 합성). 위치는 단위 정사각형 CTM(좌하단 원점 변환).

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::io::Write as _;
use std::sync::Arc;

use pdf_writer::types::{CidFontType, FontFlags, SystemInfo, TextRenderingMode, UnicodeCmap};
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};
use rustybuzz::ttf_parser;
use subsetter::GlyphRemapper;

use crate::display::{DisplayList, Item, PageList};

const ITALIC_SKEW: f32 = 0.2126;

struct FontUse {
    data: Arc<Vec<u8>>,
    index: u32,
    res_name: String,
    type0: Ref,
    cid: Ref,
    descriptor: Ref,
    file: Ref,
    cmap: Ref,
    gid_to_char: BTreeMap<u16, char>,
    /// 이 폰트로 그려진 모든 글리프 ID (서브셋 대상).
    used_gids: BTreeSet<u16>,
    /// 원본 GID → 서브셋 GID 재매핑 (서브셋 후 채움).
    remapper: GlyphRemapper,
}

/// 임베드 준비가 끝난 이미지 한 장 (XObject 스트림 바이트 + 메타).
struct ImageUse {
    res_name: String,
    id: Ref,
    width: i32,
    height: i32,
    /// 스트림 바이트 (JPEG 원본 또는 zlib 압축 RGB).
    data: Vec<u8>,
    filter: Filter,
    gray: bool,
}

/// JPEG SOF에서 (너비, 높이, 컴포넌트 수)를 읽는다 (DCTDecode 통과용).
fn jpeg_dims(data: &[u8]) -> Option<(u32, u32, u8)> {
    if data.len() < 2 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 4 <= data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        // 길이 없는 standalone 마커: RSTn(D0..D7), SOI/EOI/TEM 등.
        if (0xD0..=0xD9).contains(&marker) || marker == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
        // SOF0..SOF15 (DHT=C4, JPG=C8, DAC=CC 제외)
        let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
        if is_sof {
            if i + 9 < data.len() {
                let h = u16::from_be_bytes([data[i + 5], data[i + 6]]) as u32;
                let w = u16::from_be_bytes([data[i + 7], data[i + 8]]) as u32;
                let comps = data[i + 9];
                return Some((w, h, comps));
            }
            return None;
        }
        i += 2 + len;
    }
    None
}

/// zlib(=PDF FlateDecode) 압축.
fn deflate(data: &[u8]) -> Vec<u8> {
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = enc.write_all(data);
    enc.finish().unwrap_or_default()
}

/// 인코딩된 이미지 바이트를 PDF XObject 스트림으로 준비한다.
/// JPEG(1/3 컴포넌트)는 DCTDecode 무손실 통과, 그 외(또는 CMYK JPEG)는 RGB 디코드 후 Flate.
fn prepare_image(bytes: &[u8]) -> Option<(i32, i32, Vec<u8>, Filter, bool)> {
    if bytes.starts_with(&[0xFF, 0xD8])
        && let Some((w, h, comps)) = jpeg_dims(bytes)
        && (comps == 1 || comps == 3)
        && w > 0
        && h > 0
    {
        return Some((
            w as i32,
            h as i32,
            bytes.to_vec(),
            Filter::DctDecode,
            comps == 1,
        ));
    }
    let img = image::load_from_memory(bytes).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    // 알파를 흰 배경에 합성하고 RGB로 평탄화.
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in rgba.pixels() {
        let [r, g, b, a] = px.0;
        let af = a as f32 / 255.0;
        let comp = |c: u8| ((c as f32 * af) + 255.0 * (1.0 - af)).round() as u8;
        rgb.push(comp(r));
        rgb.push(comp(g));
        rgb.push(comp(b));
    }
    Some((
        w as i32,
        h as i32,
        deflate(&rgb),
        Filter::FlateDecode,
        false,
    ))
}

/// DisplayList를 단일 다페이지 PDF 바이트로 직렬화한다.
pub fn render_pdf(list: &DisplayList) -> Vec<u8> {
    let mut pdf = Pdf::new();
    let mut alloc: i32 = 1;
    let take = |alloc: &mut i32| {
        let r = Ref::new(*alloc);
        *alloc += 1;
        r
    };

    let catalog_id = take(&mut alloc);
    let page_tree_id = take(&mut alloc);

    // ── 1. 폰트 수집 (data 포인터로 유일화) ──
    let mut fonts: Vec<FontUse> = Vec::new();
    let mut font_index: HashMap<usize, usize> = HashMap::new();
    for page in &list.pages {
        for item in &page.items {
            if let Item::Glyphs { run, .. } = item {
                let key = run.font.data.as_ptr() as usize;
                let fi = match font_index.get(&key) {
                    Some(&i) => i,
                    None => {
                        let idx = fonts.len();
                        let f = FontUse {
                            data: run.font.data.clone(),
                            index: run.font.index,
                            res_name: format!("F{idx}"),
                            type0: take(&mut alloc),
                            cid: take(&mut alloc),
                            descriptor: take(&mut alloc),
                            file: take(&mut alloc),
                            cmap: take(&mut alloc),
                            gid_to_char: BTreeMap::new(),
                            used_gids: BTreeSet::new(),
                            remapper: GlyphRemapper::new(),
                        };
                        fonts.push(f);
                        font_index.insert(key, idx);
                        idx
                    }
                };
                // 그려진 모든 글리프를 서브셋 대상에 추가 (gid_to_char 가드 밖).
                for g in &run.glyphs {
                    fonts[fi].used_gids.insert(g.id);
                }
                let chars: Vec<char> = run.text.chars().collect();
                if chars.len() == run.glyphs.len() {
                    for (g, c) in run.glyphs.iter().zip(chars) {
                        fonts[fi].gid_to_char.entry(g.id).or_insert(c);
                    }
                }
            }
        }
    }

    // ── 1c. 이미지 수집 (data 포인터로 유일화; 준비 실패 시 건너뜀) ──
    let mut images: Vec<ImageUse> = Vec::new();
    let mut image_index: HashMap<usize, usize> = HashMap::new();
    for page in &list.pages {
        for item in &page.items {
            if let Item::Image { data, .. } = item {
                let key = data.as_ptr() as usize;
                if image_index.contains_key(&key) {
                    continue;
                }
                if let Some((width, height, bytes, filter, gray)) = prepare_image(data) {
                    let idx = images.len();
                    images.push(ImageUse {
                        res_name: format!("Im{idx}"),
                        id: take(&mut alloc),
                        width,
                        height,
                        data: bytes,
                        filter,
                        gray,
                    });
                    image_index.insert(key, idx);
                }
            }
        }
    }

    // ── 1b. 폰트별 글리프 재매핑 빌드 (정렬된 used_gids; gid 0 자동 포함) ──
    for f in &mut fonts {
        let mut rm = GlyphRemapper::new();
        for &g in &f.used_gids {
            rm.remap(g);
        }
        f.remapper = rm;
    }

    // ── 2. 폰트 임베드 (서브셋) ──
    for f in &fonts {
        let face = ttf_parser::Face::parse(&f.data, f.index).ok();
        let upem = face.as_ref().map_or(1000.0, |fc| fc.units_per_em() as f32);
        let scale = 1000.0 / upem;
        let (bx0, by0, bx1, by1) = face.as_ref().map_or((0.0, -200.0, 1000.0, 800.0), |fc| {
            let b = fc.global_bounding_box();
            (
                b.x_min as f32 * scale,
                b.y_min as f32 * scale,
                b.x_max as f32 * scale,
                b.y_max as f32 * scale,
            )
        });
        let ascent = face
            .as_ref()
            .map_or(800.0, |fc| fc.ascender() as f32 * scale);
        let descent = face
            .as_ref()
            .map_or(-200.0, |fc| fc.descender() as f32 * scale);
        let cap = face
            .as_ref()
            .and_then(|fc| fc.capital_height())
            .map_or(ascent, |c| c as f32 * scale);
        let italic_angle = face.as_ref().map_or(0.0, |fc| fc.italic_angle());

        pdf.type0_font(f.type0)
            .base_font(Name(b"EmbeddedFont"))
            .encoding_predefined(Name(b"Identity-H"))
            .descendant_font(f.cid)
            .to_unicode(f.cmap);

        let mut cid = pdf.cid_font(f.cid);
        cid.subtype(CidFontType::Type2)
            .base_font(Name(b"EmbeddedFont"))
            .system_info(SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Identity"),
                supplement: 0,
            })
            .font_descriptor(f.descriptor)
            .cid_to_gid_map_predefined(Name(b"Identity"))
            .default_width(1000.0);
        cid.finish();

        let mut desc = pdf.font_descriptor(f.descriptor);
        desc.name(Name(b"EmbeddedFont"))
            .flags(FontFlags::SYMBOLIC)
            .bbox(Rect::new(bx0, by0, bx1, by1))
            .italic_angle(italic_angle)
            .ascent(ascent)
            .descent(descent)
            .cap_height(cap)
            .stem_v(80.0)
            .font_file2(f.file);
        desc.finish();

        // 서브셋: 사용 글리프만 남긴 폰트 프로그램 (실패 시 전체 폰트로 폴백 — 예: CFF2).
        let subset = subsetter::subset(&f.data, f.index, &f.remapper)
            .map(std::borrow::Cow::Owned)
            .unwrap_or_else(|_| std::borrow::Cow::Borrowed(f.data.as_slice()));
        let len1 = subset.len() as i32;
        let mut stream = pdf.stream(f.file, &subset);
        stream.pair(Name(b"Length1"), len1);
        stream.finish();

        let mut cmap = UnicodeCmap::new(
            Name(b"Adobe-Identity-UCS"),
            SystemInfo {
                registry: Str(b"Adobe"),
                ordering: Str(b"Identity"),
                supplement: 0,
            },
        );
        for (gid, ch) in &f.gid_to_char {
            // ToUnicode 키는 서브셋 GID (= 콘텐츠 스트림이 보여주는 코드).
            if let Some(new_gid) = f.remapper.get(*gid) {
                cmap.pair(new_gid, *ch);
            }
        }
        pdf.cmap(f.cmap, &cmap.finish());
    }

    // ── 2b. 이미지 XObject 임베드 ──
    for img in &images {
        let mut xobj = pdf.image_xobject(img.id, &img.data);
        xobj.width(img.width);
        xobj.height(img.height);
        xobj.bits_per_component(8);
        xobj.filter(img.filter);
        if img.gray {
            xobj.color_space().device_gray();
        } else {
            xobj.color_space().device_rgb();
        }
        xobj.finish();
    }

    // ── 3. 페이지 ──
    let mut page_ids: Vec<Ref> = Vec::new();
    for page in &list.pages {
        let page_id = take(&mut alloc);
        let content_id = take(&mut alloc);
        page_ids.push(page_id);

        let content = build_page_content(page, &fonts, &font_index, &images, &image_index);

        let mut p = pdf.page(page_id);
        p.media_box(Rect::new(0.0, 0.0, page.width_pt, page.height_pt));
        p.parent(page_tree_id);
        p.contents(content_id);
        {
            let mut res = p.resources();
            let mut dict = res.fonts();
            for f in &fonts {
                dict.pair(Name(f.res_name.as_bytes()), f.type0);
            }
            dict.finish();
            if !images.is_empty() {
                let mut xdict = res.x_objects();
                for img in &images {
                    xdict.pair(Name(img.res_name.as_bytes()), img.id);
                }
                xdict.finish();
            }
            res.finish();
        }
        p.finish();

        pdf.stream(content_id, &content);
    }

    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(page_ids.len() as i32);
    pdf.catalog(catalog_id).pages(page_tree_id);

    pdf.finish()
}

fn build_page_content(
    page: &PageList,
    fonts: &[FontUse],
    font_index: &HashMap<usize, usize>,
    images: &[ImageUse],
    image_index: &HashMap<usize, usize>,
) -> Vec<u8> {
    let h = page.height_pt;
    let mut content = Content::new();

    for item in &page.items {
        match item {
            Item::Rect {
                x,
                y,
                w,
                h: rh,
                fill,
            } => {
                let (r, g, b) = rgb(*fill);
                content.save_state();
                content.set_fill_rgb(r, g, b);
                content.rect(*x, h - (*y + *rh), *w, *rh);
                content.fill_nonzero();
                content.restore_state();
            }
            Item::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => {
                let (r, g, b) = rgb(*color);
                content.save_state();
                content.set_stroke_rgb(r, g, b);
                content.set_line_width(*width);
                content.move_to(*x1, h - *y1);
                content.line_to(*x2, h - *y2);
                content.stroke();
                content.restore_state();
            }
            Item::Image {
                x,
                y,
                w,
                h: ih,
                data,
            } => {
                let key = data.as_ptr() as usize;
                let Some(&ii) = image_index.get(&key) else {
                    continue; // 준비 실패 이미지 — 건너뜀
                };
                // 단위 정사각형 이미지 공간 → 페이지 좌표 (좌하단 원점, y 뒤집기).
                content.save_state();
                content.transform([*w, 0.0, 0.0, *ih, *x, h - (*y + *ih)]);
                content.x_object(Name(images[ii].res_name.as_bytes()));
                content.restore_state();
            }
            Item::Glyphs { x, y, run } => {
                let Some(&fi) = font_index.get(&(run.font.data.as_ptr() as usize)) else {
                    continue;
                };
                let res_name = fonts[fi].res_name.as_bytes();
                let (r, g, b) = rgb(run.color);
                let skew = if run.italic { ITALIC_SKEW } else { 0.0 };

                content.begin_text();
                content.set_fill_rgb(r, g, b);
                if run.bold {
                    content.set_text_rendering_mode(TextRenderingMode::FillStroke);
                    content.set_stroke_rgb(r, g, b);
                    content.set_line_width(run.size_pt * 0.03);
                } else {
                    content.set_text_rendering_mode(TextRenderingMode::Fill);
                }
                content.set_font(Name(res_name), run.size_pt);

                let mut pen_x = *x;
                for glyph in &run.glyphs {
                    let gx = pen_x + glyph.x_offset;
                    let gy = h - (*y - glyph.y_offset);
                    content.set_text_matrix([run.x_scale, 0.0, skew, 1.0, gx, gy]);
                    // 서브셋 GID로 표시 (CID = 서브셋 GID, CIDToGIDMap=Identity).
                    let sub_gid = fonts[fi].remapper.get(glyph.id).unwrap_or(0);
                    content.show(Str(&sub_gid.to_be_bytes()));
                    pen_x += glyph.x_advance;
                }
                content.end_text();
            }
        }
    }
    content.finish().to_vec()
}

/// COLORREF(0x00BBGGRR) → (r,g,b) 0.0..=1.0. 없음(0xFFFFFFFF)은 검정.
fn rgb(c: u32) -> (f32, f32, f32) {
    if c == 0xFFFF_FFFF {
        return (0.0, 0.0, 0.0);
    }
    let r = (c & 0xFF) as f32 / 255.0;
    let g = ((c >> 8) & 0xFF) as f32 / 255.0;
    let b = ((c >> 16) & 0xFF) as f32 / 255.0;
    (r, g, b)
}
