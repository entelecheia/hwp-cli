# 08. 외부 근거 — OWPML 표준·오픈소스·페이지네이션 동작 (deep-research)

> 2026-07-04, 다중 소스 웹 리서치 + 적대적 검증(98 에이전트)으로 수집·검증. 우리가 실기로
> 관찰한 "도형 몰린 문단 → 글상자 드롭 + 페이지 오버플로"를 **뒷받침 또는 반증**하기 위한 외부 근거.

## 조사 질문

hwp5→hwpx 변환기에서 원본이 "한 페이지 도형 수십 개(예: 35개)를 하나의 문단(`hp:p`)에 앵커"
하는 구조일 때, 한글 렌더링에서 (a) 일부 글상자(`hp:rect`+`drawText`)가 안 그려지고 (b) 페이지가
오버플로해 빈 페이지가 삽입된다. **원본 hwp5는 정상인데 우리 hwpx 변환본만** 이 현상이 발생.

## 검증된 발견 (신뢰도·출처)

| # | 발견 | 신뢰도 | 근거 |
|---|---|---|---|
| 1 | **"한 문단에 도형 수십 개 앵커"는 정상 구조.** 표준 참조 변환기 `hwp2hwpx`도 문단을 **1:1 매핑**해 35개 도형을 한 `hp:p`에 그대로 넣고 **여러 문단으로 재분배하지 않는다**. | 높음 | neolord0/hwplib·hwp2hwpx (`ForSectionXMLFileList.section()`이 문단 1:1, `ForGso.convert()`가 같은 run에 모든 GSO append) |
| 2 | hwp5·HWPX 두 포맷 모두 도형(GSO)은 **문단 하위 앵커 개체**. 다수 개체를 한 문단에 두는 것은 스키마 위반 아님. | 높음 | hwplib(Paragraph.controlList), Hancom tech blog(hp:p→hp:run→hp:t) |
| 3 | (Q1) **문단/런/페이지당 그리기 개체 수 제한이 문서화된 곳 없음.** 개체 분산 강제 규칙도 없음. | 중간(침묵 근거) | hwp2hwpx README, hancom-io/hwpx-owpml-model |
| 4 | (Q2) **페이지네이션은 파일에 저장되지 않고 렌더 타임에 "문단 중심"으로 계산.** "문단을 그리다 영역이 부족하면 그때 페이지가 추가된다." | 높음 | hwplib README(페이지 API 미지원), 유지관리자 이슈 #31 |
| 5 | (Q3) **"도형 많은 문단에서 글상자 드롭" 버그가 문서화된 곳 없음** (변경이력·이슈·공식 모델 저장소 모두). | 중간(침묵 근거) | hwp2hwpx changelog(~2026-06-25)·이슈 7건, hancom 모델 저장소 |
| 6 | HWPX 출력은 **국가표준 KS X 6101 = OWPML** 스키마를 따라야 함. 상세 개체/z-order/렌더 규칙은 오픈소스가 아니라 외부 OWPML **상세 스펙(PDF)**에 있음. | 높음 | Hancom 공식, standard.go.kr, hancom-io 모델 저장소 |
| 7 | (Q5) hwp5=CFB 바이너리(레코드→XML 트리 변환 필요), HWPX=이미 XML(OPC/ZIP). 최소 한 렌더러(rhwp)는 둘을 **단일 통합 IR**로 파싱. | 중간 | pyhwp 문서, rhwp 온보딩(HWP/HWPX→Parser→IR→Paginator→Layout) |

## ★결정적 함의 — 원인 재구성

- **구조는 정상이다.** 표준 변환기조차 하지 않는 "도형 문단 분산"은 정석 해법이 아니다. → 우리가
  계획했던 "도형을 여러 문단에 분산"하는 재설계는 **표준과 어긋나며 근본 해법이 아닐 가능성**이 크다.
- **페이지네이션은 렌더 타임·문단 중심**이다. 빈 페이지는 저장 속성이 아니라 한글의 레이아웃 계산
  차이다. 앵커 문단의 **줄 높이·문단 속성**이 페이지 계산을 좌우한다.
- **가장 유력한 원인(열린 질문):** 변환 매핑이 구조적으로 1:1이어도, 각 개체·문단의 **속성 충실도**
  (`vertRelTo`/`horzRelTo`/`treatAsChar`/`z-order`/`textWrap`/`offset`/`vpos`, 빈 문단 줄 높이 등)가
  원본 hwp5와 출력 hwpx 사이에서 달라져 한글 레이아웃이 바뀌는 것. **속성 단위 diff는 미검증.**

## 한계 (반드시 유의)

- **Q1·Q3 답은 "문서화된 근거 없음"이라는 침묵 근거**(argument from silence). 제한이 없음을 증명하지
  못한다 — 한글 내부(폐쇄 소스) 렌더러의 **미문서화 실용 한계** 가능성은 남는다.
- 이 증상을 직접 설명하려던 4개 가설(① TopAndBottom 개체 공간 예약/vpos 보정 누락, ② treatAsChar
  개체의 LINE_SEG 높이 과다, ③ python-hwpx 하위 요소 누락, ④ shapeObject/connectLine 표현)은
  3표 적대적 검증에서 **전부 반증(0-3/1-2)**. 이 자료만으로는 메커니즘 귀속 불가.
- 출처가 오픈소스 README·표준 등록부에 치우침. 개체 앵커/공간 예약/z-order 규칙이 실제 기술될
  Hancom OWPML **상세 스펙(PDF)은 직접 열람 못 함**. rhwp는 서드파티 재구현이라 한글 실제 동작을 대변 못함.

## 후속 조사 방향 (우선순위)

1. **속성 단위 diff** — 원본 hwp5의 도형/문단 속성 vs 우리 hwpx 출력 속성을 개체별로 대조
   (vertRelTo/horzRelTo/treatAsChar/z-order/textWrap/offset/vpos, 빈 문단 char_shape·줄높이). ← 유력.
2. Hancom OWPML **상세 스펙 PDF**(hancom.com/etc/hwpDownload.do) 확보·열람 — 개체 앵커/공간 예약 규칙.
3. 35개 도형을 유지하되 **속성만 정품과 일치**시켰을 때 한글에서 드롭/오버플로가 해소되는지 실기.
4. 빈 페이지가 도형이 아니라 **앵커 문단의 줄 높이**에서 오는지 — 빈 문단 줄 높이를 정품과 맞춰 검증.

## 참고 출처

- neolord0/hwplib, neolord0/hwp2hwpx (사실상 표준 참조 변환기)
- hancom-io/hwpx-owpml-model (공식 OWPML 모델)
- Hancom tech blog (tech.hancom.com/hwpxformat, python-hwpx-parsing)
- KS X 6101 (국가표준, standard.go.kr) — OWPML 문서 구조
- pyhwp 문서, rhwp (서드파티 통합 렌더러)
