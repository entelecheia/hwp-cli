# Changelog

모든 주요 변경은 이 파일에 기록한다. 형식은 [Keep a Changelog](https://keepachangelog.com/),
버전은 [SemVer](https://semver.org/)를 따른다.

## [0.2.0]

### Added
- **PDF 출력** (`convert --to pdf`, `render --format pdf`) — 텍스트 선택가능 벡터 PDF.
  CIDFontType2(Identity-H) 글리프 임베드 + 사용 글리프 서브셋, 래스터 이미지 XObject 임베드
  (JPEG DCTDecode 통과, PNG/BMP/GIF FlateDecode RGB).
- **HTML 출력** (`convert --to html`, `cat --format html`) — 표·제목·굵게/기울임/밑줄/취소선.
- **ODT 출력** (`convert --to odt`) — 단락/제목/표/이미지/메타데이터(내용 충실도, 단방향).
- **문서 메타데이터** — 제목/지은이/주제/키워드 읽기·쓰기. hwp5 `\x05HwpSummaryInformation`,
  hwpx `Contents/content.hpf`(OPF). `new --title/--author/--subject`, `edit --set-meta`,
  `info`/`info --json` 노출, HTML `<title>` 연동.
- **도형 렌더** — gso 도형을 회색 외곽선/형태(사각형·타원·선)로, 수식/차트/OLE은 외곽선+X
  placeholder로 렌더(이전엔 무음 드롭). PNG/SVG/PDF 공통.
- **`--strict`** — `convert`에서 보존 불가(드롭) 데이터 발견 시 비정상 종료(hwp/hwpx 대상).
- **메모(주석) 작성** — `edit --add-memo`(hwpx 출력 전용, 실험적). hwp 출력은 경고 후 생략.
- **`slots`/`fill`/`validate`** 서브커맨드.
- **MCP 도구 3종 추가** — `hwp_slots`, `hwp_fill`, `hwp_validate` (총 11종). `hwp_convert`가
  html/pdf/odt 출력 지원.

### Changed
- 워크스페이스 버전 0.1.0 → 0.2.0.
- `pdf.rs`: 폰트 전체 임베드 → 사용 글리프 서브셋. `Item::Image` 미지원 → XObject 임베드.

### Notes
- PDF/HTML/ODT는 단방향 내보내기로 페이지 레이아웃(여백·단)까지 1:1 재현하지 않는다.
- 메모 작성은 실험적(OWPML `<hp:memogroup>` 최선 노력) — 한컴오피스 실열기 검증 권장.
- 도형은 렌더만 가능하며 의미 모델·포맷 간 합성은 없다.

## [0.1.0]

- 최초 릴리스: hwp/hwpx 읽기·쓰기, markdown/JSON 변환, PNG/SVG 렌더, hwp 바이너리 바이트 동일
  왕복, 누름틀/필드 채우기, MCP 서버(8종).
