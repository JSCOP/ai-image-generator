# MCP 구현 진행 기록

## 2026-09-14 — 초기 진단

- 기존 진입점은 `tools/ai_image.py` CLI, `tools/image_studio.py` HTTP UI, 수동 크롭 UI로 분리되어 있다. MCP 프로토콜 서버/클라이언트 설정은 없다.
- CLI single 모드는 `dry_run`을 처리하지 않는다. MCP 계획 확인에서 그대로 호출하면 실생성을 유발한다.
- preset 배치는 실행 시 난수로 계획을 다시 생성한다. MCP에서는 계획을 파일에 고정한 후 동일한 jobs를 실행해야 한다.
- 공통 작업 ID, 영속 상태, 취소, 재시작 후 재개, 결과 이미지 MCP 반환이 없다.
- 기존 `ImageGallery/`, `debug_events.json`은 사용자 미추적 파일이며 변경하지 않는다.

## 구현 범위

공식 Python MCP SDK의 stdio 서버, 모델/연결 조회, typed 생성 계획(single/jobs/batch), 영속 비동기 실행·조회·취소·재개, 프리셋 조회/생성, 이미지 검색·메타데이터·미리보기, 크롭·리사이즈·컨택트시트, 기존 Studio 기록 조회. 기본 파일 접근은 저장소 아래이며 추가 작업 루트는 서버 인자로 지정한다. API 키는 기존 환경변수만 사용한다.

## 검증 계획

외부 과금 없이 로컬 provider fixture와 실제 MCP stdio client로 handshake, 도구/리소스, 생성/편집/배치, 실패·취소·재개, 경로 검증과 기존 회귀 테스트를 확인한다. 최종 검사 결과와 남은 문제는 여기에 기록한다.

## 최종 검증 — 2026-09-14

- `py_compile` — MCP 모델/서비스/서버 통과.
- 실제 Python MCP stdio client — initialize 통과, 도구 20개 조회, `plan_generation` 성공. 계획 단계 provider 호출 없음.
- 기존 회귀 테스트 — 25개 모두 통과.
- 직접 확인한 결함 및 수정: MCP 진입점 부재 → `mcp_server.py`; 작업 상태/취소/재개 부재 → 파일 기반 state와 process-tree 취소; 결과 검증 부재 → 출력 존재와 요청 픽셀 검사; 외부 참조 경로 제한 부재 → allowed roots와 이미지 크기 검증; 배치 재생성 비결정성 → plan에서 seed로 jobs를 고정; API 키 노출 위험 → 환경변수만 사용하고 상태/오류를 scrub.

## 남은 문제

- 실제 이미지 provider 호출은 API 키/크레딧과 네트워크가 필요하므로 이 검증에서는 실행하지 않았다.
- MCP 서버는 하나의 `--state-dir`를 한 프로세스가 소유한다. 여러 독립 MCP 클라이언트가 동시에 필요하면 서로 다른 `--state-dir`를 지정해야 한다.
- 계획된 single/jobs는 MCP가 `jobs` 명세로 실행한다. 이는 기존 `tools/ai_image.py`를 재사용하기 위한 의도적인 정규화이며 출력 결과와 재개 semantics는 동일하다.

## Gallery storage update

Image outputs now live under ImageGallery/output/<topic>; plans and job status live under ImageGallery/metadata/_mcp. Generation specifications are passed on stdin instead of creating a second specs file. Per-topic locks and all crop/resize/contact-sheet records are outside output.

## 다른 PC 설치 검증 — 2026-10-07

Windows의 별도 clone(공백 포함 경로)과 새 사용자 홈/.venv에서 `scripts/setup.py --agent codex --agent claude --mcp`, 동일 옵션의 `--check`, 링크 점검이 통과했습니다. Python 3.13.5, Pillow 12.3.0, MCP SDK 1.30.0 환경에서 회귀 테스트 33개 및 실제 MCP stdio initialize/도구 20개 조회/`plan_generation` 호출이 통과했습니다. 외부 생성 API 호출은 하지 않았습니다. 신규 설치/업데이트 절차는 [../INSTALL.md](../INSTALL.md)를 따릅니다.
