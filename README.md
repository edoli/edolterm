# edolterm

GPUI와 `alacritty_terminal`로 만든 Rust 터미널입니다. Windows에서 먼저 구현·검증하며, OS 연동과 공통 UI/터미널 코드를 분리했습니다.

## 빌드와 실행

Windows의 Rust MSVC 툴체인, Visual Studio C++ Build Tools, Windows SDK가 필요합니다. 릴리스 빌드에는 GPUI의 셰이더 컴파일을 위한 `fxc.exe`도 필요하며, 기본 Windows SDK 설치 위치에서 자동으로 탐색합니다.

```powershell
cargo build --release --target-dir target
.\target\release\edolterm.exe
```

GPUI와 `gpui_platform`은 crates.io 패키지가 아닌 공식 `zed-industries/zed` 저장소의 커밋 `b5cd310b3b54dfc659fcc06453e6358843dfe216`을 사용합니다. 2026-11-08 구현 시작 시 확인한 최신 HEAD이며, 재현 가능한 빌드를 위해 `rev`와 `Cargo.lock`을 고정했습니다. 업데이트할 때 두 GPUI 의존성의 커밋을 함께 바꾸세요.

## 기능

- 타이틀바와 통합한 사각형 탭, 활성 탭 상단 강조선
- 가용 너비 균등 분배 → 최소 100px → 가로 스크롤, 스크롤바와 휠 지원
- 탭 순서 드래그 변경, 가운데 클릭 닫기, 제목 툴팁
- `+` 기본 프로필 실행, `∨` 프로필 및 설정 메뉴
- PowerShell 7, Windows PowerShell, CMD, WSL 배포판 탐색
- ConPTY 셸 입출력, 크기 변경, ANSI 색상, 대체 화면, 커서, 마우스 보고
- 한글 IME 조합 표시와 확정 입력, 고정 셀 기반 문자 렌더링
- 일반/단어/줄/사각형 선택, 복사·붙여넣기, bracketed paste
- 스크롤백 및 유니코드 텍스트 검색, 이전/다음 찾기
- 설정 탭, 다크/라이트 테마, 글꼴·크기·스크롤백 설정
- 창 위치·크기와 탭의 프로필·시작 폴더 복원
- 창 종료 확인 및 UI 스레드를 막지 않는 PTY 정리

## 조작

| 조작 | 단축키 |
|---|---|
| 새 탭 | Ctrl+Shift+T |
| 현재 탭 닫기 | Ctrl+Shift+W |
| 다음 / 이전 탭 | Ctrl+Tab / Ctrl+Shift+Tab |
| 프로필 1–9 실행 | Ctrl+Shift+1–9 |
| 프로필 메뉴 | Ctrl+Shift+P |
| 설정 | Ctrl+, |
| 복사 / 붙여넣기 | Ctrl+Shift+C / Ctrl+Shift+V |
| 검색 | Ctrl+Shift+F |
| 다음 / 이전 검색 결과 | Enter / Shift+Enter |
| 검색 닫기 | Esc |
| 스크롤백 탐색 | Shift+PageUp / PageDown / Home / End |
| 전체 화면 | F11 |

`Ctrl+C`는 셸에 그대로 전달됩니다. 우클릭은 붙여넣기입니다. 마우스 보고를 사용하는 TUI에서는 `Shift`를 누르면 텍스트를 선택하거나 스크롤백을 탐색할 수 있습니다. 두 번 클릭은 단어 선택, 세 번 클릭은 줄 선택, Alt+드래그는 사각형 선택입니다.

macOS용 코드에서는 앱 단축키의 Ctrl 대신 Command를 사용하도록 분리했지만, macOS와 Linux의 실제 빌드·실행은 아직 검증하지 않았습니다.

## 설정과 사용자 지정 프로필

Windows 설정 파일은 `%APPDATA%\edolterm\settings.toml`, 세션 파일은 같은 폴더의 `session.json`입니다. 테스트나 포터블 사용 시 `EDOLTERM_CONFIG_DIR`로 별도 폴더를 지정할 수 있습니다.

```toml
default_profile = "my-pwsh"
font_family = "Cascadia Mono"
font_size = 15.0
scrollback = 10000
theme = "dark"
confirm_close = true
restore_session = true
allow_osc52 = false

[[profiles]]
id = "my-pwsh"
name = "개발 PowerShell"
program = 'C:\Program Files\PowerShell\7\pwsh.exe'
args = ["-NoLogo"]
cwd = 'C:\Users\edoli\workspace'

[profiles.env]
MY_PROJECT = "development"
```

설정 파일을 편집한 뒤 설정 탭에서 **다시 불러오기**를 누르세요. 같은 ID의 사용자 프로필은 탐색된 프로필을 덮어씁니다. 실행 파일과 인자는 별도로 저장하며, 인자에 공백이 있어도 직접 문자열을 이어 붙이지 않습니다.

글꼴 크기는 8–48px, 스크롤백은 0–1,000,000줄을 지원합니다. 터미널 색상·글꼴 설정은 저장 즉시 열린 탭에도 반영됩니다. 프로필 변경은 다음 셸 실행부터 적용됩니다.

설정이 잘못되면 파일을 자동으로 덮어쓰지 않고 오류를 표시합니다. 설정/세션 저장은 같은 디렉터리의 임시 파일을 동기화한 후 원자적으로 교체합니다. OSC52는 기본적으로 꺼져 있고, 활성화해도 터미널 출력에서 클립보드로 복사하는 방향만 허용합니다.

## 검증

```powershell
cargo fmt --check
cargo test -- --test-threads=1
cargo clippy --all-targets -- -D warnings
```

테스트에는 실제 Windows ConPTY에서 PowerShell을 실행해 유니코드 출력, 73×19 크기 변경, 입력, 종료 정리를 확인하는 통합 테스트가 포함됩니다.

디버그 빌드에는 별도 설정 폴더를 사용하는 앱 내부 스모크 테스트도 있습니다.

```powershell
cargo build
.\target\debug\edolterm.exe --smoke-test=menu
```

모드는 `terminal`, `menu`, `settings`, `overflow`, `find`, `confirm`입니다. GPUI 키 이벤트, 탭 순서 변경, 설정 저장/재로드, IME 조합 API, 실제 PTY 유니코드 출력, 검색과 선택을 확인하고 `target/ui-smoke/<모드>/result.json`에 결과를 기록합니다. 화면을 확인한 뒤 해당 폴더에 `stop` 파일을 만들면 정상 종료합니다. 재실행 전 이전 `stop` 파일을 지우세요. 이 테스트 경로는 릴리스 빌드에 포함되지 않습니다.

전체 화면 상태를 자동 검증하고 GPU 렌더링 이미지를 저장하려면 다음을 실행하세요. OS 키보드·마우스 입력을 주입하지 않으며 사용자 설정과 분리된 폴더만 사용합니다.

```powershell
.\scripts\smoke.ps1
```

## 현재 범위

세션 복원은 탭 구성과 **시작 시점의 폴더**를 복원하고 셸을 새로 실행합니다. 실행 중인 명령, 출력, 실행 후 바뀐 작업 폴더는 복원하지 않습니다. WSL은 우선 배포판의 홈에서 시작합니다.

분할 화면, 창 간 탭 이동, 셸 연동을 통한 현재 폴더 추적, 고급 키보드 프로토콜, 이미지 프로토콜, ConPTY DLL 별도 배포, 설치 프로그램과 자동 업데이트는 후속 범위입니다. 한글 조합 경로는 앱 내부 API로 검증했으며 실제 Microsoft IME 후보창 조작과 다양한 TUI/서드파티 글꼴 조합은 추가 수동 검증이 필요합니다.
