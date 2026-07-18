//! IR → GFM markdown.
//!
//! 매핑 규칙:
//! - "개요 N" 스타일 문단 → `#` × N 헤딩
//! - 문자 모양 굵게/기울임 → `**`/`*` 스팬, 취소선 → `~~`, 밑줄·위/아래첨자 →
//!   `<u>`·`<sup>`·`<sub>` (char_shape_runs 기반)
//! - 하이퍼링크(%hlk 필드) → `[표시텍스트](URL)`
//! - 이미지(Picture) → `![image]()` (또는 media_dir 지정 시 추출·상대참조)
//! - 표 → GFM 표 (첫 행을 헤더로). 병합 셀(col_span/row_span>1)이나 셀 안 중첩 표가
//!   있으면 HTML `<table>`(colspan/rowspan + 인라인 HTML 태그)로 폴백 — 내용 보존 우선
//! - 글머리표/번호 문단 → `- `/`N. ` 목록 (번호는 numbering_levels 형식 합성;
//!   아라비아 숫자 외 형식은 `- 가. ` 식 리터럴 마커로 보존)
//! - 각주/미주 → 본문 `[^N]`/`[^eN]` 마커 + 문서 끝 정의 (GFM 풋노트)
//! - 수식(eqed) → 인라인 `$스크립트$`, 블록 `$$스크립트$$` (HWP 수식 스크립트 원문)
//! - 줄나눔(10) → 강제 줄바꿈, 탭 → 공백

use std::path::Path;

use hwp_model::list::ListState;
use hwp_model::{
    Cell, CharShape, Control, Document, Equation, GenericControl, HwpChar, Paragraph, Table,
    TextOptions, ctrl_char,
};

/// markdown 출력 옵션.
#[derive(Default)]
pub struct MarkdownOptions<'a> {
    /// 이미지 바이너리를 추출할 디렉터리. `Some`이면 이미지를 `image1.png` 식으로
    /// 그 디렉터리에 뽑고 `![image](접두사/image1.png)`로 참조한다(디렉터리는
    /// 첫 이미지에서 지연 생성 — 이미지가 없으면 만들지 않는다). `None`이면 기존처럼
    /// 빈 참조 `![image]()`를 유지한다(동작 불변).
    pub media_dir: Option<&'a Path>,
    /// 이미지 참조 경로 접두사. `None`이면 `media_dir`의 디렉터리명을 쓴다(기존 동작).
    /// CLI `--media-dir figs`처럼 사용자가 준 상대경로를 링크에 그대로 쓸 때 지정한다.
    pub media_prefix: Option<&'a str>,
    /// 텍스트 추출 옵션(머리말/꼬리말·숨은 설명 포함 여부). 기본은 제외.
    pub text: TextOptions,
}

/// IR 전체를 GFM markdown으로 직렬화한다(기존 시그니처 유지 — 이미지 미추출).
pub fn to_markdown(doc: &Document) -> String {
    // media_dir 미지정 → IO가 없어 실패할 수 없다.
    to_markdown_with(doc, &MarkdownOptions::default())
        .expect("media_dir 미지정 시 IO가 없어 실패할 수 없다")
}

/// 옵션을 받는 변형. `media_dir` 지정 시 이미지를 추출하며, 추출 IO 실패는 `Err`.
pub fn to_markdown_with(doc: &Document, opts: &MarkdownOptions) -> std::io::Result<String> {
    let mut ctx = Ctx {
        media_dir: opts.media_dir,
        dir_name: opts
            .media_prefix
            .map(|p| p.trim_end_matches('/').to_string())
            .or_else(|| {
                opts.media_dir
                    .and_then(|d| d.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
            })
            .unwrap_or_default(),
        img_no: 0,
        error: None,
        include_header_footer: opts.text.include_header_footer,
        include_hidden: opts.text.include_hidden,
        html_mode: false,
        last_was_list: false,
        notes: Vec::new(),
        foot_n: 0,
        end_n: 0,
    };

    let mut out = String::new();
    for section in &doc.sections {
        // 목록 번호 카운터는 구역 단위로 리셋한다(렌더러와 같은 규칙).
        let mut list_state = ListState::default();
        for para in &section.paragraphs {
            render_paragraph(doc, para, &mut list_state, &mut ctx, &mut out);
        }
    }
    // 각주/미주 정의는 문서 끝에 모은다.
    if !ctx.notes.is_empty() {
        if !out.is_empty() && !out.ends_with("\n\n") {
            out.push('\n');
        }
        for (label, text) in &ctx.notes {
            let mut lines = text.lines();
            match lines.next() {
                Some(first) => {
                    out.push_str(&format!("[^{label}]: {first}\n"));
                    // 후속 줄은 4칸 들여쓰기(GFM 풋노트 연속 줄 규칙).
                    for l in lines {
                        out.push_str(&format!("    {l}\n"));
                    }
                }
                None => out.push_str(&format!("[^{label}]:\n")),
            }
        }
    }
    if let Some(e) = ctx.error {
        return Err(e);
    }
    Ok(cleanup(&out))
}

/// 렌더 중 상태(이미지 추출 진행·텍스트 포함 정책·목록/각주·HTML 표 모드).
struct Ctx<'a> {
    media_dir: Option<&'a Path>,
    /// 참조 경로 접두사(media_prefix 또는 디렉터리명).
    dir_name: String,
    /// 다음 이미지 번호(1-기반 카운터).
    img_no: usize,
    /// 첫 IO 오류(있으면 to_markdown_with가 Err 반환).
    error: Option<std::io::Error>,
    include_header_footer: bool,
    include_hidden: bool,
    /// HTML 표 안 — 블록 HTML에선 md가 렌더되지 않으므로 마크·링크·이미지를 HTML 태그로 방출.
    html_mode: bool,
    /// 직전 출력이 목록 항목 — 목록 블록 종료 시 빈 줄 확보용.
    last_was_list: bool,
    /// 각주/미주 (라벨, 본문) — 문서 끝 정의용.
    notes: Vec<(String, String)>,
    foot_n: u32,
    end_n: u32,
}

impl Ctx<'_> {
    /// Picture 바이트를 media_dir에 뽑고 markdown 이미지 참조를 만든다.
    /// media_dir이 없거나 추출에 실패하면 빈 참조 `![image]()`를 유지한다.
    fn image_ref(&mut self, data: &[u8]) -> String {
        let html = self.html_mode;
        let fallback = || {
            if html {
                "<!-- image -->".to_string()
            } else {
                "![image]()".to_string()
            }
        };
        let Some(dir) = self.media_dir else {
            return fallback();
        };
        self.img_no += 1;
        let (ext, _) = crate::image::image_kind(data);
        let file = format!("image{}.{ext}", self.img_no);
        // 첫 이미지에서 디렉터리 지연 생성(이미지 없으면 만들지 않음).
        if let Err(e) = std::fs::create_dir_all(dir) {
            self.record_err(e);
            return fallback();
        }
        if let Err(e) = std::fs::write(dir.join(&file), data) {
            self.record_err(e);
            return fallback();
        }
        if html {
            format!("<img src=\"{}/{}\" alt=\"image\">", self.dir_name, file)
        } else {
            format!("![image]({}/{})", self.dir_name, file)
        }
    }

    fn record_err(&mut self, e: std::io::Error) {
        if self.error.is_none() {
            self.error = Some(e);
        }
    }
}

/// 과도한 빈 줄을 정리한다.
fn cleanup(out: &str) -> String {
    let mut cleaned = String::with_capacity(out.len());
    let mut blank_run = 0;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        cleaned.push_str(line);
        cleaned.push('\n');
    }
    cleaned
}

/// 인라인 문자 효과 상태. 열기 순서 bold→italic→strike→underline→sup/sub, 닫기는 역순.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
struct Marks {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    sup: bool,
    sub: bool,
}

impl Marks {
    fn from_shape(shape: Option<&CharShape>) -> Self {
        shape.map_or(Self::default(), |s| Self {
            bold: s.is_bold(),
            italic: s.is_italic(),
            strike: s.has_strike(),
            underline: s.has_underline(),
            sup: s.is_superscript(),
            sub: s.is_subscript(),
        })
    }

    fn open(self, html: bool) -> String {
        let mut s = String::new();
        if self.bold {
            s.push_str(if html { "<b>" } else { "**" });
        }
        if self.italic {
            s.push_str(if html { "<i>" } else { "*" });
        }
        if self.strike {
            s.push_str(if html { "<s>" } else { "~~" });
        }
        if self.underline {
            s.push_str("<u>");
        }
        if self.sup {
            s.push_str("<sup>");
        }
        if self.sub {
            s.push_str("<sub>");
        }
        s
    }

    fn close(self, html: bool) -> String {
        let mut s = String::new();
        if self.sub {
            s.push_str("</sub>");
        }
        if self.sup {
            s.push_str("</sup>");
        }
        if self.underline {
            s.push_str("</u>");
        }
        if self.strike {
            s.push_str(if html { "</s>" } else { "~~" });
        }
        if self.italic {
            s.push_str(if html { "</i>" } else { "*" });
        }
        if self.bold {
            s.push_str(if html { "</b>" } else { "**" });
        }
        s
    }
}

/// 열린 마크를 전부 닫고 상태를 리셋한다(링크 경계·줄바꿈 등 강제 경계).
/// 이후 Text 문자가 오면 모양 전환 로직이 다시 연다.
fn close_marks(body: &mut String, marks: &mut Marks, html: bool) {
    body.push_str(&marks.close(html));
    *marks = Marks::default();
}

fn render_paragraph(
    doc: &Document,
    para: &Paragraph,
    list_state: &mut ListState,
    ctx: &mut Ctx,
    out: &mut String,
) {
    // 목록 번호는 문서 순서대로 모든 문단에 대해 갱신한다(빈 문단도 카운트 — 렌더와 동일).
    let marker = list_state.marker(doc, para);
    // 개요 스타일 → 헤딩
    let heading = doc
        .header
        .styles
        .get(para.style.0 as usize)
        .and_then(|s| s.name.strip_prefix("개요 "))
        .and_then(|n| n.trim().parse::<usize>().ok())
        .filter(|n| (1..=6).contains(n));

    let body = render_inline(doc, para, ctx, out);
    let body = body.trim_end();

    if let Some(level) = heading {
        close_list_block(ctx, out);
        if !body.is_empty() {
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(body);
            out.push_str("\n\n");
        }
        return;
    }

    if marker.is_some() {
        if !body.is_empty() {
            let (ty, level) = list_head(doc, para).unwrap_or((3, 1));
            let indent = "  ".repeat(level.saturating_sub(1) as usize);
            let mk = marker.as_deref().unwrap_or("-");
            if ty == 3 {
                // 불릿 — 원 문자(•/◦/■ 등)는 GFM 불릿으로 대체한다.
                out.push_str(&format!("{indent}- {body}\n"));
            } else if is_digit_marker(mk) {
                out.push_str(&format!("{indent}{mk} {body}\n"));
            } else {
                // 아라비아 숫자 외 번호 형식(가./①/제1조 등)은 GFM 목록 마커가
                // 없어 리터럴 마커로 보존한다.
                out.push_str(&format!("{indent}- {mk} {body}\n"));
            }
            ctx.last_was_list = true;
        }
        return;
    }

    close_list_block(ctx, out);
    if !body.is_empty() {
        out.push_str(body);
        out.push_str("\n\n");
    }
}

/// 목록 블록 종료 — 항목 뒤 일반 문단/헤딩 앞에 빈 줄을 확보한다.
fn close_list_block(ctx: &mut Ctx, out: &mut String) {
    if ctx.last_was_list && !out.ends_with("\n\n") {
        out.push('\n');
    }
    ctx.last_was_list = false;
}

/// (머리 종류, 수준) — 2=번호, 3=글머리표. 목록이 아니면 None.
fn list_head(doc: &Document, para: &Paragraph) -> Option<(u8, u8)> {
    let ps = doc.header.para_shapes.get(para.para_shape.0 as usize)?;
    let ty = ps.head_type();
    (ty == 2 || ty == 3).then(|| (ty, ps.head_level()))
}

/// "1." 같이 아라비아 숫자+마침표 마커(GFM 순서 목록으로 쓸 수 있는 형태)인지.
fn is_digit_marker(mk: &str) -> bool {
    let digits = mk.strip_suffix('.').unwrap_or(mk);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

/// 문단의 인라인 내용을 렌더링해 반환한다.
/// 표 등 블록 컨트롤은 out에 직접 쓴다 (문단 텍스트와 분리).
fn render_inline(doc: &Document, para: &Paragraph, ctx: &mut Ctx, out: &mut String) -> String {
    let mut body = String::new();
    let mut wchar_pos = 0u32;
    let mut marks = Marks::default();
    // 하이퍼링크 필드 열림 상태(대상 URL). FIELD_START에서 채우고 FIELD_END에서 닫는다.
    let mut link_url: Option<String> = None;

    for ch in &para.chars {
        // 현재 위치의 문자 모양으로 효과 전환
        // (중첩 정합성을 위해 변경 시 전부 닫고 다시 연다)
        if let HwpChar::Text(_) = ch {
            let want = Marks::from_shape(shape_at(doc, para, wchar_pos));
            if want != marks {
                body.push_str(&marks.close(ctx.html_mode));
                body.push_str(&want.open(ctx.html_mode));
                marks = want;
            }
        }
        match ch {
            HwpChar::Text(c) => {
                if ctx.html_mode {
                    push_html_escaped(&mut body, *c);
                } else {
                    body.push(*c);
                }
            }
            HwpChar::CharCtrl(code) => match *code {
                ctrl_char::LINE_BREAK => {
                    close_marks(&mut body, &mut marks, ctx.html_mode);
                    body.push_str(if ctx.html_mode { "<br>" } else { "  \n" });
                }
                ctrl_char::HYPHEN => body.push('-'),
                ctrl_char::NB_SPACE | ctrl_char::FW_SPACE => body.push(' '),
                _ => {}
            },
            HwpChar::InlineCtrl { code, .. } => {
                if *code == ctrl_char::FIELD_END {
                    // 하이퍼링크 표시 텍스트 종료 → `](URL)`/`</a>`로 닫는다.
                    if let Some(url) = link_url.take() {
                        close_marks(&mut body, &mut marks, ctx.html_mode);
                        if ctx.html_mode {
                            body.push_str("</a>");
                        } else {
                            body.push_str("](");
                            body.push_str(&md_link_dest(&url));
                            body.push(')');
                        }
                    }
                } else if *code == ctrl_char::TAB {
                    body.push(' ');
                }
            }
            HwpChar::ExtCtrl {
                code, ctrl_index, ..
            } => {
                if let Some(idx) = ctrl_index
                    && let Some(control) = para.controls.get(*idx as usize)
                {
                    if *code == ctrl_char::FIELD_START
                        && let Some(url) = crate::field::hyperlink_url(control)
                    {
                        // 하이퍼링크 필드 시작 → `[`/`<a href>` 방출, 이후 표시 텍스트를 링크로 묶는다.
                        close_marks(&mut body, &mut marks, ctx.html_mode);
                        if ctx.html_mode {
                            body.push_str("<a href=\"");
                            for c in url.chars() {
                                match c {
                                    '&' => body.push_str("&amp;"),
                                    '"' => body.push_str("&quot;"),
                                    '<' => body.push_str("&lt;"),
                                    _ => body.push(c),
                                }
                            }
                            body.push_str("\">");
                        } else {
                            body.push('[');
                        }
                        link_url = Some(url);
                    } else {
                        render_control(doc, control, *code, ctx, &mut body, out);
                    }
                }
            }
        }
        wchar_pos += ch.wchar_width();
    }
    body.push_str(&marks.close(ctx.html_mode));
    body
}

/// markdown 링크 대상 포맷: 공백·괄호가 있으면 `<...>`로 감싼다.
fn md_link_dest(url: &str) -> String {
    if url
        .chars()
        .any(|c| c.is_whitespace() || c == '(' || c == ')')
    {
        format!("<{}>", url.replace('<', "%3C").replace('>', "%3E"))
    } else {
        url.to_string()
    }
}

fn render_control(
    doc: &Document,
    control: &Control,
    code: u16,
    ctx: &mut Ctx,
    body: &mut String,
    out: &mut String,
) {
    match control {
        Control::SectionDef(_) => {}
        Control::Picture(pic) => match doc.resolve_bin(&pic.bin_ref) {
            Some(data) => {
                let r = ctx.image_ref(data);
                body.push_str(&r);
            }
            None => body.push_str(if ctx.html_mode {
                "<!-- image -->"
            } else {
                "![image]()"
            }),
        },
        Control::Table(table) => {
            // 병합 셀·셀 안 중첩 표는 GFM 파이프 표로 표현 불가 → HTML 표 폴백.
            if ctx.html_mode || has_span(table) || has_nested_table(table) {
                render_html_table(doc, table, ctx, out);
            } else {
                render_gfm_table(doc, table, ctx, out);
            }
        }
        Control::Generic(g) => {
            // 수식 → $스크립트$ (원문 보존).
            if let Some(eq) = &g.equation {
                render_equation(eq, ctx, body, out);
                return;
            }
            // 각주/미주 → 본문 `[^N]` 마커 + 문서 끝 정의 (본문 인라인 흡수 대체).
            if code == ctrl_char::FOOTNOTE_ENDNOTE && matches!(&g.ctrl_id, b"fn  " | b"en  ") {
                let label = if g.ctrl_id == *b"fn  " {
                    ctx.foot_n += 1;
                    ctx.foot_n.to_string()
                } else {
                    ctx.end_n += 1;
                    format!("e{}", ctx.end_n)
                };
                let text = note_text(doc, g, ctx);
                ctx.notes.push((label.clone(), text));
                body.push_str(&format!("[^{label}]"));
                return;
            }
            // 머리말/꼬리말·숨은설명은 옵션에 따라 제외 (텍스트 추출 정책과 동일).
            if (code == ctrl_char::HEADER_FOOTER && !ctx.include_header_footer)
                || (code == ctrl_char::HIDDEN_COMMENT && !ctx.include_hidden)
            {
                return;
            }
            for list in &g.paragraph_lists {
                for p in &list.paragraphs {
                    let mut sub_out = String::new();
                    let inline = render_inline(doc, p, ctx, &mut sub_out);
                    let inline = inline.trim();
                    if !inline.is_empty() {
                        if !body.is_empty() && !body.ends_with([' ', '\n']) {
                            body.push(' ');
                        }
                        body.push_str(inline);
                    }
                    out.push_str(&sub_out);
                }
            }
        }
    }
}

/// 수식 — 인라인은 `$..$`, 블록은 `$$..$$` (HWP 수식 스크립트 원문, LaTeX 아님).
fn render_equation(eq: &Equation, ctx: &mut Ctx, body: &mut String, out: &mut String) {
    if ctx.html_mode {
        body.push_str("<code>");
        for c in eq.script.chars() {
            push_html_escaped(body, c);
        }
        body.push_str("</code>");
    } else if eq.inline {
        body.push('$');
        body.push_str(&eq.script);
        body.push('$');
    } else {
        // 블록 수식은 표와 같은 경로로 out에 직접 쓴다(문단 텍스트와 분리).
        out.push_str("\n$$\n");
        out.push_str(&eq.script);
        out.push_str("\n$$\n\n");
    }
}

/// 각주/미주 본문 — 문단들을 인라인으로 렌더해 합친다(블록은 줄 단위로 뒤에 붙임).
fn note_text(doc: &Document, g: &GenericControl, ctx: &mut Ctx) -> String {
    let mut parts: Vec<String> = Vec::new();
    for list in &g.paragraph_lists {
        for p in &list.paragraphs {
            let mut sub = String::new();
            let inline = render_inline(doc, p, ctx, &mut sub);
            let inline = inline.trim();
            if !inline.is_empty() {
                parts.push(inline.to_string());
            }
            let sub = sub.trim();
            if !sub.is_empty() {
                parts.push(sub.to_string());
            }
        }
    }
    parts.join("\n")
}

/// 병합 셀(가로/세로)이 하나라도 있으면 GFM 파이프 표로 표현 불가.
fn has_span(table: &Table) -> bool {
    table.cells.iter().any(|c| c.col_span > 1 || c.row_span > 1)
}

/// 셀 안에 중첩 표가 있으면 GFM 파이프 표로 표현 불가.
fn has_nested_table(table: &Table) -> bool {
    table.cells.iter().any(|c| {
        c.paragraphs
            .iter()
            .any(|p| p.controls.iter().any(|ct| matches!(ct, Control::Table(_))))
    })
}

/// GFM 파이프 표 (첫 행 헤더, 병합 없음).
fn render_gfm_table(doc: &Document, table: &Table, ctx: &mut Ctx, out: &mut String) {
    let cols = table.cols.max(1) as usize;
    let mut grid: Vec<Vec<String>> = Vec::new();
    for cell in &table.cells {
        let row = cell.row as usize;
        while grid.len() <= row {
            grid.push(vec![String::new(); cols]);
        }
        let mut text = String::new();
        for p in &cell.paragraphs {
            let mut cell_out = String::new();
            let inline = render_inline(doc, p, ctx, &mut cell_out);
            if !text.is_empty() && !inline.is_empty() {
                text.push(' ');
            }
            text.push_str(inline.trim());
        }
        if let Some(slot) = grid[row].get_mut(cell.col as usize) {
            *slot = text.replace('|', "\\|").replace('\n', " ");
        }
    }
    out.push('\n');
    for (i, row) in grid.iter().enumerate() {
        out.push_str("| ");
        out.push_str(&row.join(" | "));
        out.push_str(" |\n");
        if i == 0 {
            out.push_str(&format!("|{}\n", " --- |".repeat(cols)));
        }
    }
    out.push('\n');
}

/// HTML 표 — 병합 셀(colspan/rowspan)·셀 내 블록(중첩 표 포함)을 보존한다.
/// 블록 HTML 안에선 md가 렌더되지 않으므로 셀 내용은 html_mode로 방출한다.
fn render_html_table(doc: &Document, table: &Table, ctx: &mut Ctx, out: &mut String) {
    let rows = table.rows.max(1) as usize;
    let cols = table.cols.max(1) as usize;
    // 병합 셀이 덮는 칸 표시 격자.
    let mut covered = vec![vec![false; cols]; rows];
    out.push_str("\n<table>\n");
    for r in 0..rows {
        out.push_str("<tr>");
        for c in 0..cols {
            if covered[r][c] {
                continue; // 앞선 병합 셀이 덮은 칸
            }
            let Some(cell) = table
                .cells
                .iter()
                .find(|cell| cell.row as usize == r && cell.col as usize == c)
            else {
                out.push_str("<td></td>");
                continue;
            };
            for dr in 0..cell.row_span.max(1) as usize {
                for dc in 0..cell.col_span.max(1) as usize {
                    if let Some(slot) = covered.get_mut(r + dr).and_then(|row| row.get_mut(c + dc))
                    {
                        *slot = true;
                    }
                }
            }
            let mut attrs = String::new();
            if cell.col_span > 1 {
                attrs.push_str(&format!(" colspan=\"{}\"", cell.col_span));
            }
            if cell.row_span > 1 {
                attrs.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
            }
            let content = render_cell_html(doc, cell, ctx);
            out.push_str(&format!("<td{attrs}>{content}</td>"));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n\n");
}

/// 셀 내용을 html_mode 인라인으로 렌더(문단 사이는 `<br>`, 중첩 표 등 블록은 뒤에).
fn render_cell_html(doc: &Document, cell: &Cell, ctx: &mut Ctx) -> String {
    let saved = ctx.html_mode;
    ctx.html_mode = true;
    let mut texts: Vec<String> = Vec::new();
    let mut blocks = String::new();
    for p in &cell.paragraphs {
        let inline = render_inline(doc, p, ctx, &mut blocks);
        let inline = inline.trim();
        if !inline.is_empty() {
            texts.push(inline.to_string());
        }
    }
    ctx.html_mode = saved;
    let mut content = texts.join("<br>");
    let blocks = blocks.trim();
    if !blocks.is_empty() {
        if !content.is_empty() {
            content.push_str("<br>");
        }
        content.push_str(blocks);
    }
    content
}

/// HTML 텍스트 노드 이스케이프 (& < >).
fn push_html_escaped(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        _ => out.push(c),
    }
}

/// 주어진 WCHAR 위치의 문자 모양.
fn shape_at<'d>(doc: &'d Document, para: &Paragraph, pos: u32) -> Option<&'d CharShape> {
    let id = para
        .char_shape_runs
        .iter()
        .rev()
        .find(|(start, _)| *start <= pos)
        .map(|(_, id)| *id)?;
    doc.header.char_shapes.get(id.0 as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::from_markdown::from_markdown;
    use hwp_model::{CharShapeId, HwpChar, ParagraphList};

    /// 하이퍼링크가 md→IR→md 왕복에서 `[표시](URL)`로 보존한다.
    /// (링크 표시 텍스트는 from_markdown이 밑줄 서식을 주므로 `<u>`가 붙는다.)
    #[test]
    fn 하이퍼링크_왕복_보존() {
        let doc = from_markdown("자세히는 [여기](https://example.com/path)를 본다\n");
        let md = to_markdown(&doc);
        assert!(
            md.contains("[<u>여기</u>](https://example.com/path)"),
            "링크 왕복: {md}"
        );
    }

    /// media_dir 미지정이면 이미지 참조는 빈 참조를 유지한다(동작 불변).
    #[test]
    fn 이미지_기본은_빈참조() {
        let mut doc = from_markdown("사진: 여기");
        let png = png_bytes();
        crate::image::insert_image(
            &mut doc,
            "사진:",
            &write_temp("md_img_none.png", &png),
            crate::image::ImageSize::Natural,
        )
        .unwrap();
        let md = to_markdown(&doc);
        assert!(md.contains("![image]()"), "빈 참조 유지: {md}");
    }

    /// media_dir 지정 시 이미지가 디렉터리에 추출되고 상대경로로 참조된다.
    #[test]
    fn 이미지_media_dir_추출() {
        let mut doc = from_markdown("사진: 여기");
        let png = png_bytes();
        crate::image::insert_image(
            &mut doc,
            "사진:",
            &write_temp("md_img_extract.png", &png),
            crate::image::ImageSize::Natural,
        )
        .unwrap();

        let dir = unique_dir("md_media_extract");
        // 추출 전에는 디렉터리가 없어야 한다(지연 생성 확인).
        assert!(!dir.exists());
        let md = to_markdown_with(
            &doc,
            &MarkdownOptions {
                media_dir: Some(&dir),
                ..Default::default()
            },
        )
        .unwrap();
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(
            md.contains(&format!("![image]({name}/image1.png)")),
            "상대경로 참조: {md}"
        );
        let extracted = dir.join("image1.png");
        assert!(extracted.exists(), "이미지 파일 추출");
        assert_eq!(std::fs::read(&extracted).unwrap(), png, "추출 바이트 일치");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 이미지가 없으면 media_dir 지정이어도 디렉터리를 만들지 않는다.
    #[test]
    fn 이미지_없으면_디렉터리_미생성() {
        let doc = from_markdown("본문만 있는 문단\n");
        let dir = unique_dir("md_media_empty");
        let _ = to_markdown_with(
            &doc,
            &MarkdownOptions {
                media_dir: Some(&dir),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!dir.exists(), "이미지 없으면 디렉터리 미생성");
    }

    /// 이미지 여러 개가 등장 순서대로 image1/image2로 번호 매겨진다.
    #[test]
    fn 이미지_카운터_증가() {
        // 두 이미지가 순서대로 image1/image2로 번호 매겨진다.
        let mut doc = from_markdown("첫 사진: 여기\n\n둘 사진: 저기");
        let png = png_bytes();
        crate::image::insert_image(
            &mut doc,
            "첫 사진:",
            &write_temp("md_cnt1.png", &png),
            crate::image::ImageSize::Natural,
        )
        .unwrap();
        crate::image::insert_image(
            &mut doc,
            "둘 사진:",
            &write_temp("md_cnt2.png", &png),
            crate::image::ImageSize::Natural,
        )
        .unwrap();
        let dir = unique_dir("md_media_counter");
        let md = to_markdown_with(
            &doc,
            &MarkdownOptions {
                media_dir: Some(&dir),
                ..Default::default()
            },
        )
        .unwrap();
        let name = dir.file_name().unwrap().to_string_lossy();
        assert!(md.contains(&format!("{name}/image1.png")), "첫 이미지");
        assert!(md.contains(&format!("{name}/image2.png")), "둘째 이미지");
        assert!(dir.join("image2.png").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// media_prefix 지정 시 이미지 참조가 디렉터리명 대신 접두사 경로를 쓴다.
    #[test]
    fn 이미지_media_prefix() {
        let mut doc = from_markdown("사진: 여기");
        let png = png_bytes();
        crate::image::insert_image(
            &mut doc,
            "사진:",
            &write_temp("md_img_prefix.png", &png),
            crate::image::ImageSize::Natural,
        )
        .unwrap();
        let dir = unique_dir("md_media_prefix");
        let md = to_markdown_with(
            &doc,
            &MarkdownOptions {
                media_dir: Some(&dir),
                media_prefix: Some("figs"),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            md.contains("![image](figs/image1.png)"),
            "prefix 참조: {md}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 각주/미주가 본문 `[^N]`/`[^eN]` 마커 + 문서 끝 정의로 방출된다 (GH-3).
    #[test]
    fn 각주_미주_마커와_정의() {
        let mut doc = from_markdown("본문 문단\n");
        let note = |id: &[u8; 4], txt: &str| {
            Control::Generic(GenericControl {
                ctrl_id: *id,
                data: vec![],
                paragraph_lists: vec![ParagraphList {
                    header_data: vec![],
                    paragraphs: vec![Paragraph {
                        chars: txt.chars().map(HwpChar::Text).collect(),
                        ..Paragraph::default()
                    }],
                }],
                extras: vec![],
                raw_children: vec![],
                gso_shapes: vec![],
                equation: None,
                column_def: None,
            })
        };
        let anchor = |idx: u32, id: &[u8; 4]| HwpChar::ExtCtrl {
            code: ctrl_char::FOOTNOTE_ENDNOTE,
            ctrl_id: *id,
            payload: vec![],
            ctrl_index: Some(idx),
        };
        let para = &mut doc.sections[0].paragraphs[0];
        // 첫 문단에는 구역 정의 컨트롤이 선주입돼 있으므로 현재 개수가 곧 인덱스다.
        let i0 = para.controls.len() as u32;
        para.controls.push(note(b"fn  ", "각주 내용"));
        let i1 = para.controls.len() as u32;
        para.controls.push(note(b"en  ", "미주 내용"));
        para.chars.push(anchor(i0, b"fn  "));
        para.chars.push(anchor(i1, b"en  "));

        let md = to_markdown(&doc);
        assert!(md.contains("본문 문단[^1][^e1]"), "본문 마커: {md}");
        assert!(md.contains("[^1]: 각주 내용"), "각주 정의: {md}");
        assert!(md.contains("[^e1]: 미주 내용"), "미주 정의: {md}");
        // 정의는 문서 끝에 모인다.
        assert!(
            md.trim_end().ends_with("[^e1]: 미주 내용"),
            "정의는 문서 끝: {md}"
        );
    }

    /// 수식이 인라인은 `$..$`, 블록은 `$$..$$`로 방출된다.
    #[test]
    fn 수식_인라인_블록() {
        let mk = |script: &str, inline: bool| {
            Control::Generic(GenericControl {
                ctrl_id: *b"eqed",
                data: vec![],
                paragraph_lists: vec![],
                extras: vec![],
                raw_children: vec![],
                gso_shapes: vec![],
                equation: Some(Equation {
                    script: script.to_string(),
                    width: 0,
                    height: 0,
                    inline,
                    x: 0,
                    y: 0,
                }),
                column_def: None,
            })
        };
        let anchor = |idx: u32| HwpChar::ExtCtrl {
            code: ctrl_char::OBJECT,
            ctrl_id: *b"eqed",
            payload: vec![],
            ctrl_index: Some(idx),
        };
        let mut doc = from_markdown("인라인 수식: \n");
        let p0 = &mut doc.sections[0].paragraphs[0];
        // 첫 문단의 선주입 구역 정의 컨트롤 뒤 인덱스를 쓴다.
        let i0 = p0.controls.len() as u32;
        p0.chars.push(anchor(i0));
        p0.controls.push(mk("a+b", true));
        // 블록 수식만 있는 문단 추가.
        doc.sections[0].paragraphs.push(Paragraph {
            chars: vec![anchor(0)],
            controls: vec![mk("x^2", false)],
            ..Paragraph::default()
        });

        let md = to_markdown(&doc);
        assert!(md.contains("$a+b$"), "인라인 수식: {md}");
        assert!(md.contains("$$\nx^2\n$$"), "블록 수식: {md}");
    }

    /// 글머리표/번호 문단이 GFM 목록으로 방출된다 (GH-6).
    #[test]
    fn 목록_불릿과_번호() {
        use hwp_model::{NumFmt, NumLevel, ParaShape, ParaShapeId};
        let ps = |ty: u32, lv: u32, nid: u16| ParaShape {
            attr1: (ty << 23) | (lv << 25),
            numbering_id: nid,
            ..ParaShape::default()
        };
        // 숫자 번호 목록 — 카운터는 numbering id를 가리지 않고 공유(렌더와 같은 규칙)라
        // 형식이 다른 목록은 문서를 나눠 검증한다.
        let mut doc = from_markdown("불릿 하나\n\n번호 하나\n\n번호 둘\n");
        let base = doc.header.para_shapes.len() as u16;
        doc.header.para_shapes.push(ps(3, 1, 0)); // 불릿
        doc.header.para_shapes.push(ps(2, 1, 0)); // 번호(숫자)
        doc.header.bullet_chars = vec!['•'];
        doc.header.numbering_levels = vec![vec![NumLevel::default(); 7]];
        for (i, p) in doc.sections[0].paragraphs.iter_mut().enumerate() {
            p.para_shape = ParaShapeId(if i == 0 { base } else { base + 1 });
        }
        let md = to_markdown(&doc);
        assert!(md.contains("- 불릿 하나\n"), "불릿: {md}");
        assert!(md.contains("1. 번호 하나\n"), "숫자 번호 1: {md}");
        assert!(md.contains("2. 번호 둘\n"), "숫자 번호 2: {md}");

        // 가나다 형식 번호는 GFM 목록 마커가 없어 리터럴 마커로 보존한다.
        let mut doc2 = from_markdown("한글 번호\n");
        let base2 = doc2.header.para_shapes.len() as u16;
        doc2.header.para_shapes.push(ps(2, 1, 1));
        doc2.header.numbering_levels = vec![
            vec![NumLevel::default(); 7],
            vec![
                NumLevel {
                    start: 1,
                    fmt: NumFmt::HangulSyllable,
                    template: String::new(),
                };
                7
            ],
        ];
        doc2.sections[0].paragraphs[0].para_shape = ParaShapeId(base2);
        let md2 = to_markdown(&doc2);
        assert!(md2.contains("- 가. 한글 번호"), "한글 형식 리터럴: {md2}");
    }

    /// 병합 셀이 있으면 HTML 표(colspan/rowspan)로 폴백한다 (GH-4).
    #[test]
    fn 병합셀_html_표() {
        use hwp_model::{BorderFillId, Cell, HwpUnit, Table};
        let cell = |row: u16, col: u16, cs: u16, rs: u16, txt: &str| Cell {
            list_attr: 0,
            col,
            row,
            col_span: cs,
            row_span: rs,
            width: HwpUnit(0),
            height: HwpUnit(0),
            margins: [0; 4],
            border_fill: BorderFillId(0),
            header_tail: vec![],
            paragraphs: vec![Paragraph {
                chars: txt.chars().map(HwpChar::Text).collect(),
                ..Paragraph::default()
            }],
        };
        let table = Table {
            common_data: vec![],
            placement: None,
            attr: 0,
            rows: 2,
            cols: 2,
            cell_spacing: 0,
            inner_margins: [0; 4],
            row_cell_counts: vec![1, 2],
            border_fill: BorderFillId(0),
            table_tail: vec![],
            cells: vec![
                cell(0, 0, 2, 1, "병합"),
                cell(1, 0, 1, 1, "가"),
                cell(1, 1, 1, 1, "나"),
            ],
            extras: vec![],
        };
        let mut doc = from_markdown("표 문단\n");
        let p = &mut doc.sections[0].paragraphs[0];
        // 첫 문단의 선주입 구역 정의 컨트롤 뒤 인덱스를 쓴다.
        let i0 = p.controls.len() as u32;
        p.chars.push(HwpChar::ExtCtrl {
            code: ctrl_char::OBJECT,
            ctrl_id: *b"tbl ",
            payload: vec![],
            ctrl_index: Some(i0),
        });
        p.controls.push(Control::Table(table));

        let md = to_markdown(&doc);
        assert!(md.contains("<table>"), "HTML 표: {md}");
        assert!(md.contains("<td colspan=\"2\">병합</td>"), "colspan: {md}");
        assert!(md.contains("<td>가</td><td>나</td>"), "나머지 행: {md}");
    }

    /// 밑줄/취소선/위·아래첨자가 스팬으로 방출된다.
    #[test]
    fn 글자효과_스팬() {
        let mut doc = from_markdown("효과\n");
        let shapes = [
            CharShape {
                attr: 1 << 2, // 밑줄(글자 아래)
                ..CharShape::default()
            },
            CharShape {
                strike: true,
                ..CharShape::default()
            },
            CharShape {
                attr: 1 << 15, // 위첨자
                ..CharShape::default()
            },
            CharShape {
                attr: 1 << 16, // 아래첨자
                ..CharShape::default()
            },
        ];
        let base = doc.header.char_shapes.len() as u16;
        doc.header.char_shapes.extend(shapes);
        let para = &mut doc.sections[0].paragraphs[0];
        para.chars = "ABCD".chars().map(HwpChar::Text).collect();
        para.char_shape_runs = (0..4)
            .map(|i| (i as u32, CharShapeId(base + i as u16)))
            .collect();

        let md = to_markdown(&doc);
        assert!(
            md.contains("<u>A</u>~~B~~<sup>C</sup><sub>D</sub>"),
            "효과 스팬: {md}"
        );
    }

    fn png_bytes() -> Vec<u8> {
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend([0, 0, 0, 13]);
        png.extend(b"IHDR");
        png.extend(96u32.to_be_bytes());
        png.extend(96u32.to_be_bytes());
        png.extend([0u8; 8]);
        png
    }

    fn write_temp(name: &str, data: &[u8]) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(name);
        std::fs::write(&p, data).unwrap();
        p
    }

    fn unique_dir(stem: &str) -> std::path::PathBuf {
        let uniq = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{stem}_{uniq}"))
    }
}
