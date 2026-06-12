# hwp-cli

한글 문서(.hwp, .hwpx)를 처리하는 Rust CLI. HWP 5.0 바이너리와 HWPX(OWPML,
KS X 6101) 포맷의 읽기·쓰기·변환·렌더링을 외부 HWP 라이브러리 없이 직접 구현한다.

## 목표 기능

- **읽기·텍스트 추출** — hwp/hwpx → plain/markdown/JSON
- **포맷 변환** — hwp → hwpx, hwpx ↔ markdown/JSON
- **이미지 렌더링** — hwp/hwpx → PNG/SVG/PDF (파일에 저장된 줄 배치
  정보(PARA_LINE_SEG)를 활용해 원본에 가까운 레이아웃)
- **문서 생성·쓰기** — hwpx와 **hwp 바이너리 쓰기** (생태계 공백)

## 사용법

```sh
hwp info <file>                     # 포맷/버전/속성/스트림 진단
hwp cat <file> [--format md|json]   # 본문 텍스트 추출 (--preview: PrvText)
hwp convert <in> -o out.md          # hwp/hwpx → markdown/JSON
hwp convert <in> -o out.hwpx        # hwp/hwpx → hwpx (표·이미지·머리말 보존)
hwp render <in> -o out.png          # 페이지 렌더링 (PNG/SVG, --dpi, --font-dir)
hwp new -o out.hwpx --from doc.md   # markdown → 새 hwpx 문서
hwp dump <file> [--raw] [--json]    # [개발자용] 레코드/패키지 구조 덤프
```

렌더링은 표(테두리/배경), 이미지, 머리말/꼬리말, 밑줄/취소선을 지원하며
파일에 저장된 줄 배치(lineseg)를 우선 사용하고 불완전한 파일은 자체
줄바꿈으로 보정한다. hwp 바이너리 쓰기(M6)와 PDF 출력(M7)은 진행 중.

## 워크스페이스 구성

| 크레이트 | 역할 |
|---|---|
| `hwp-model` | 공유 문서 모델(IR) — 모든 크레이트의 계약 |
| `hwp5` | HWP 5.0 바이너리 reader/writer (CFB + 레코드 스트림) |
| `hwpx` | HWPX reader/writer (ZIP + OWPML XML) |
| `hwp-convert` | IR ↔ markdown/JSON |
| `hwp-render` | IR → PNG/SVG/PDF 렌더러 |
| `hwp-cli` | `hwp` 바이너리 |

## 개발

```sh
cargo build
cargo test
cargo clippy --all-targets
```

`docs/`에 한글문서파일형식 5.0 공식 스펙 PDF와 스펙 hwp 원본(배포용 문서
테스트 겸용)이 있다. 진행 상황과 설계 결정은 계획 문서(마일스톤 M0~M7) 참조.

## 라이선스

MIT OR Apache-2.0
