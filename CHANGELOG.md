# Changelog

모든 주요 변경은 이 파일에 기록한다. 형식은 [Keep a Changelog](https://keepachangelog.com/),
버전은 [SemVer](https://semver.org/)를 따른다.

## [Unreleased]

### Added
- **표 행 추가(양식 변형)** — `hwp_convert::add_rows(doc, table_index, template_row, count)`:
  마지막의 병합 없는 행을 복제해 빈 행 N개를 추가(셀 폭·여백·테두리·문자/문단 모양 보존,
  내용은 비움)하고, 추가된 행은 `set_cell`로 채운다. IR 구조체 변경 없음.
  - `hwp edit --add-row "표:N"`(또는 `"표:템플릿행:N"`) — `--set-cell`보다 먼저 적용돼
    같은 호출에서 새 행을 채운다.
  - `hwp fill --data {"fields":{...}, "tables":[{"table":0,"start_row":1,"rows":[[..],..]}]}`
    — 데이터 수만큼 표를 자동 증식 + 셀 채우기(.hwp/.hwpx). 자리표시자 치환(바이트 보존)
    경로는 그대로 유지.
  - MCP `hwp_edit`에 `add_rows` 배열(set_cell보다 먼저 적용).
  - 가드: 병합 셀/세로병합에 덮인 부분 행은 복제 거부(그리드 보호); u16 행 한도 초과 거부;
    복제 문단은 고유 instance_id·마지막 문단 bit31 부여(hwp5 출신 편집 경로는 writer가
    정규화하지 않으므로 IR에서 보장). hwp5 바이트 동일성 게이트는 미편집 경로만 영향받아
    무관, 한글 합성 게이트(셀당 nparas≥1, row_cell_counts 정합)는 통과.

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

### Fixed
- **가로 용지 미반영** — `PAGE_DEF` 가로(landscape) 비트를 무시해 가로 문서를 세로로
  렌더, 제목·표 우측 열이 잘리던 문제. 가로일 때 폭/높이를 교환하고 본문 폭을 재계산
  (한컴 PrvImage 대조: 운영현황 자료 가로 표 복원).
- **셀 세로 정렬 무시** — 표 셀의 세로 정렬(가운데/아래) 속성을 무시해 모든 셀 내용이
  상단 정렬되던 문제. 셀 내용 실측 높이로 가운데/아래 오프셋을 적용(라벨/값 셀 세로 가운데).
- **대체 글꼴 굵기 손실** — 굵게(글자모양 bold 비트) 또는 굵은 글꼴명(견고딕/헤드라인/Black
  등)인데 대체 글꼴엔 굵은 페이스가 없어 제목·라벨이 가늘게 렌더되던 문제. 합성 굵게
  (글리프 윤곽선 위 중앙 스트로크, 크기의 4.5%)를 PNG/SVG/PDF 공통 적용(한컴 굵게 대조 보정).
- **가짜 취소선** — HWP5 글자모양 취소선 비트(18~20, DIFFSPEC)를 신뢰하던 탓에 한컴이
  평문으로 렌더하는 본문에 가짜 가로줄이 그어지던 문제(한컴 PrvImage 대조 확인). 명시적
  `CharShape.strike` 플래그로 분리(HWPX visible strikeout만 적용).
- **글리프 두부(□)** — 문서가 지정한 글꼴(예: macOS 시스템 "휴먼명조")에 없는 기호(❍ U+274D 등)가
  .notdef로 렌더되던 문제. 글자별 커버리지 폴백(함초롬 → Noto/Nanum)으로 조각을 분할.
- **표 행 겹침** — 행 높이를 저장된 `cell.height`(한컴 줄바꿈 기준)로만 잡아, 우리 줄바꿈이 더 많은
  줄을 만들면 다중행 셀이 다음 행을 침범하던 문제. 셀 내용 실측 패스로 `max(저장, 실측)` 높이 적용.

### Notes
- PDF/HTML/ODT는 단방향 내보내기로 페이지 레이아웃(여백·단)까지 1:1 재현하지 않는다.
- 메모 작성은 실험적(OWPML `<hp:memogroup>` 최선 노력) — 한컴오피스 실열기 검증 권장.
- 도형은 렌더만 가능하며 의미 모델·포맷 간 합성은 없다.

## [0.1.0]

- 최초 릴리스: hwp/hwpx 읽기·쓰기, markdown/JSON 변환, PNG/SVG 렌더, hwp 바이너리 바이트 동일
  왕복, 누름틀/필드 채우기, MCP 서버(8종).
