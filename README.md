# Challa

Windows용 텍스트 익스팬더입니다. 영문 단축어를 한글, 여러 줄 문장과 서명으로 바꿉니다.

- 다크 테마 스니펫 그리드, 검색, 편집, 사용 중지와 삭제
- 즉시, 스페이스, Enter, Tab과 구분 문자 트리거
- 키보드 훅과 치환 작업 분리, 입력 순서 보존과 모디파이어 처리
- Windows 11 메모장 RichEdit의 커서 앞 단축어 확인 후 원자적 교체
- Windows 표준 입력창은 클립보드 없이 교체, 다른 입력창은 클립보드를 백업한 뒤 붙여넣기
- Windows 계정에 묶인 DPAPI 암호화 파일 저장
- 확인된 비밀번호 입력창에서 등록된 단축어 치환, 알 수 없는 입력창 상태에서는 추적 중지

단축어는 영문, 숫자와 기호로 등록합니다. 두벌식 한글 입력도 같은 키의 단축어에 대응하므로 `#rksk`를 등록하면 `#가나`로 입력해도 처리합니다. 기본 단축어는 즉시 실행하는 `:challa`와 스페이스로 실행하는 `:sig`입니다. 스니펫 편집 화면은 전역 치환 대상에서 제외합니다.

외부 앱의 한국어 IME는 해당 앱의 IME 상태 창을 통해 처리합니다. 한글 조합을 정리한 뒤 원래 변환 모드를 복원합니다. 비밀번호 입력창에서는 클립보드를 사용하지 않고 Unicode 키 입력으로 치환합니다. 치환 실패 시 하단에 IME, 단축어 위치, 포커스 변경 등의 원인을 표시합니다.

## 생산성 기능

- 단축어 편집 중 중복, 접두어와 접미어, 겹치는 구분 문자 실행 조건을 안내합니다. 같은 단축어와 실행 방식은 공통 또는 같은 프로필에서 중복 저장할 수 없습니다.
- 준비 상태는 사용 가능, 빈칸 입력, 다른 프로필, 엔진 일시 정지, 보관함 잠김과 필드 준비 필요를 구분합니다. 상태 버튼으로 프로필, 보관함 또는 엔진 복구 작업을 엽니다.
- 다른 앱의 일반 입력창에서 `Ctrl+Alt+Space`를 눌러 빠른 호출창을 엽니다. 이름과 단축어로 검색하고 방향키, Enter로 입력합니다. Esc는 취소합니다. 100개까지 표시하며 검색으로 범위를 좁힐 수 있습니다. 단축키가 다른 앱에서 사용 중이면 안내합니다.
- 편집기의 내용 유형에서 `빈칸 템플릿`을 선택하고 `{{고객명}}`, `{{날짜}}`처럼 작성합니다. 단축어 실행 또는 빠른 호출에서 값을 채워 완성 문구를 확인합니다. 날짜 빈칸에는 오늘 날짜를 채웁니다. 일반 텍스트의 중괄호는 원문대로 입력합니다.
- 공통 스니펫은 모든 프로필에서 사용합니다. 업무와 개인 프로필은 작업 프로필 설정과 트레이에서 선택합니다. `outlook.exe`와 같은 실행 파일 이름에 자동 선택 규칙을 지정할 수 있습니다. 앱 규칙이 기본 프로필보다 우선하며 웹사이트별 권한과는 별도입니다.
- 원래 입력창, 프로세스와 포커스를 다시 확인한 뒤 입력합니다. 빠른 호출창에서 다른 창으로 이동하면 요청을 취소합니다. 보관함이 잠기거나 엔진이 정지하면 진행 중 입력도 취소합니다. 요청 유효 시간은 5분입니다.
- 창을 닫으면 트레이에서 계속 실행합니다. 트레이 메뉴에서 업데이트 확인, 열기, 프로필(업무와 개인), 일시 정지 / 재개, 보관함 잠금과 종료를 사용할 수 있습니다.

스니펫의 프로필과 템플릿 설정은 기존 `snippets.dpapi`에 함께 저장합니다. 이전 스니펫은 공통 프로필과 일반 텍스트로 읽습니다. 프로필 규칙은 같은 설정 폴더의 `settings.dpapi`에 Windows DPAPI로 암호화합니다. Bitwarden 참조에는 빈칸 템플릿을 적용하지 않습니다.

검색 구조는 전체 목록에서 한 번에 생성하고, 저장이 성공한 후 짧게 교체합니다. 키보드 훅은 불변 스냅샷을 사용합니다. 관리 화면과 빠른 호출창은 코드를 나누어 불러오며, 화면 상태는 변경 이벤트와 창 복귀 시 조회로 갱신합니다. Windows 세션 보호 확인은 계속 수행합니다.

## Bitwarden 연결

공식 Bitwarden Password Manager CLI를 사용합니다. CLI는 별도로 설치하고 PATH에 등록합니다. npm 설치도 지원합니다.

로그인 입력값 앞뒤의 공백은 정리합니다. 오류는 서버 설정, 기존 로그인 상태, API 키 인증 거절, TLS, 통신과 암호화 프로필 저장 단계로 구분합니다. CLI 오류 원문은 비밀값이 포함될 수 있으므로 화면이나 로그에 전달하지 않고 고정된 안내로 변환합니다.

```powershell
npm install -g @bitwarden/cli
```

1. Challa의 Bitwarden 보관함을 엽니다. 서버를 선택하고 개인 API 키의 Client ID와 Client Secret으로 로그인합니다.
2. 마스터 비밀번호로 잠금을 해제합니다. 인증 입력은 제출 후 화면에서 지우며, 비밀번호와 세션 키를 디스크에 저장하지 않습니다.
3. 항목과 필드를 선택하고 `스니펫으로 연결`을 누릅니다. 단축어와 실행 방식을 지정해 저장합니다. 편집기의 `내용 유형`에서도 Bitwarden 필드를 선택할 수 있습니다.
4. 다른 앱에서 단축어를 입력합니다. 잠긴 보관함이나 준비되지 않은 필드는 단축어를 그대로 유지합니다. 확인된 비밀번호 입력창에서도 등록된 단축어를 사용할 수 있습니다.

항목 선택 목록은 `항목명 (아이디)`로 표시합니다. 이름과 아이디, 여러 검색어와 한글 초성으로 검색하고 방향키와 Enter로 선택합니다. 검색은 이미 읽은 목록을 로컬에서 필터링합니다. 잠금 해제와 동기화 시 여러 참조를 한 번에 준비하며, 기존 참조와 항목 요약은 세션 캐시를 재사용합니다.

지원 필드는 사용자 이름, 비밀번호, 표준 TOTP, 보안 메모와 텍스트 또는 숨김 사용자 지정 필드입니다. TOTP는 입력 시점에 계산합니다. 마스터 비밀번호 재확인 항목과 연결형 사용자 지정 필드는 자동 치환하지 않습니다.

Challa는 설정 폴더의 `bitwarden-cli/`를 별도 CLI 프로필로 사용합니다. 기존 CLI 계정과 데스크톱 앱의 로그인 상태는 변경하지 않습니다. CLI가 관리하는 인증 상태와 암호화된 보관함 데이터에는 Windows DPAPI를 한 번 더 적용해 `state.dpapi`에 저장합니다. CLI 실행 중에만 `active/data.json`을 작업용으로 만들고, 작업이 끝나면 삭제합니다. 강제 종료로 남은 작업용 파일은 다음 CLI 실행 전에 정리하고 마지막 암호화 상태에서 복구합니다.

Challa의 세션 키와 선택한 필드 값은 Rust 메모리에만 보관합니다. 스니펫 파일에는 항목 ID와 필드 참조만 저장합니다. 사용 중에는 잠금 해제 상태를 유지하고, Windows 세션이 잠기면 메모리 세션을 잠급니다. Windows 잠금 상태는 1초 간격으로 확인합니다. 세션 키는 디스크에 남지 않으므로 PC를 다시 시작하거나 Challa를 종료하면 보관함은 잠긴 상태로 시작합니다. 동기화는 준비된 값을 먼저 지운 뒤 다시 조회합니다.

일반 스니펫은 잠금 상태에서도 작동합니다. 동기화나 CLI 실행은 키보드 훅에서 수행하지 않습니다. 브라우저 미리보기는 실제 보관함에 연결하지 않고 인증 입력도 받지 않습니다.

공식 Password Manager SDK가 외부 사용을 지원하지 않아 공식 CLI를 사용합니다. 근거: [Bitwarden SDK](https://github.com/bitwarden/sdk-internal), [CLI 인증과 세션](https://bitwarden.com/help/cli/).

## 시스템 잠금과 엔진 복구

- Windows를 잠그면 Challa 보관함도 잠깁니다. 열려 있던 세션의 키만 Windows DPAPI로 암호화하여 메모리에 보관하고, 평문 필드 캐시를 지웁니다.
- 같은 앱 실행 중 Windows 잠금을 해제하면 이전 보관함 세션을 복원하고, 등록된 단축어의 필드를 다시 준비합니다. 직접 잠근 보관함은 잠금 상태를 유지합니다. 앱 재시작 후에는 마스터 비밀번호로 잠금을 해제합니다.
- Windows 잠금 해제 후 입력 훅을 다시 설치합니다. 사용자가 선택한 일시 정지 상태는 유지합니다.
- 엔진 실행 중 단축어가 멈추면 앱 하단 또는 트레이 메뉴의 **엔진 복구**를 누릅니다. 입력 훅, 치환 작업 스레드와 입력 상태를 새로 만들고 단축어를 재개합니다.

이 동작이나 관련 화면을 변경한 뒤에는 프로젝트 루트에서 `cargo test --workspace`, `pnpm build`를 실행합니다. 격리된 로컬 미리보기(`pnpm preview --host 127.0.0.1 --port 4188`)에서 `node scripts/test_engine_recovery.mjs http://127.0.0.1:4188`를 실행합니다. Node.js, Playwright와 설치된 브라우저가 필요합니다. 이 검사는 모의 IPC만 사용하며 실제 금고와 Windows 설정을 변경하지 않습니다. 종료 코드 0과 `docs/engine-recovery-ui/results.json`의 `passed: true`를 확인합니다. 실패 원인을 수정하고 재검사한 후 설치 파일을 만듭니다. 실제 PC 잠금과 로그인 연동은 별도로 확인합니다.

## 실행과 빌드

Windows 배포용 앱은 처음 실행할 때 자동 실행을 등록합니다. Windows 로그인으로 시작하면 관리 창을 띄우지 않고 트레이에서 실행합니다. `설정`에서 자동 실행과 닫기 버튼의 트레이 동작을 변경할 수 있습니다. 개발 빌드와 격리된 테스트는 Windows 자동 실행 등록을 변경하지 않습니다.

트레이 아이콘을 왼쪽 클릭하면 관리 창을 복원합니다. 오른쪽 메뉴에서 업데이트 확인, 열기, 프로필 변경, 엔진 정지와 재개, 보관함 잠금과 앱 종료를 실행합니다. 닫기 버튼은 기본적으로 창을 트레이에 숨깁니다. 완전히 종료하려면 트레이 메뉴의 `종료`를 선택합니다. 트레이 생성에 실패하면 창을 숨기지 않도록 처리합니다.

Node.js, pnpm, Rust와 Windows WebView2 Runtime이 필요합니다.

```powershell
pnpm install
pnpm run build
cargo build -p challa
./target/debug/challa.exe
```

로컬 확인용 release 실행 파일은 프론트엔드 빌드 후 `cargo build --release -p challa`로 만듭니다. 결과는 `target/release/challa.exe`입니다. 배포는 아래 릴리스 스크립트가 만드는 NSIS 설치 파일로만 합니다.

## 설정 폴더

모든 설정과 데이터는 `%USERPROFILE%\.config\challa`에만 저장합니다. 다른 폴더를 대신 사용하지 않습니다.

| 파일 | 내용 |
|---|---|
| `snippets.dpapi` | 스니펫 |
| `settings.dpapi` | 프로필 규칙 |
| `desktop.dpapi` | 자동 실행과 트레이 설정 |
| `bitwarden-cli/state.dpapi` | Challa 전용 Bitwarden CLI 상태 |

## 로컬 릴리스 빌드와 배포

GitHub 릴리스에는 NSIS 설치 파일(`Challa_<version>_x64-setup.exe`), 업데이트 서명(`.sig`)과 `latest.json`만 게시합니다. `challa.exe` 실행 파일은 게시하지 않습니다. 로컬 릴리스 스크립트로 버전 일치 확인, Windows 자격 증명 관리자 서명 키 주입, NSIS 인스톨러 번들 빌드, `latest.json` 생성 및 GitHub 릴리스 업로드를 수행합니다.

```powershell
# 배포 없이 빌드 및 latest.json 산출물 검증만 수행할 때
uv run python scripts/release.py --dry-run

# 실제 GitHub Release 생성 및 에셋 업로드 수행 시
uv run python scripts/release.py
```

### 성공 판단 기준
1. `Cargo.toml` workspace, `package.json`, `src-tauri/tauri.conf.json` 세 파일의 버전이 일치합니다.
2. Windows 자격 증명 관리자에 저장된 `challa-signing_key`와 `challa-signing_password`가 정상적으로 로드됩니다.
3. `pnpm tauri build`가 성공하고 `target/release/bundle/nsis/` 아래에 NSIS 설치 파일(`*.exe`)과 서명 파일(`*.sig`)이 생성됩니다.
4. `target/release/bundle/latest.json`이 생성되고 플랫폼별 다운로드 URL(`https://github.com/cloim/Challa/releases/download/v<version>/...`)과 서명이 올바르게 기재됩니다.
5. (실제 배포 시) `gh release create`로 태그와 함께 설치 파일, `.sig`, `latest.json`이 GitHub 릴리스에 업로드됩니다.

### 실패 시 조치
- **버전 불일치 오류**: `Cargo.toml`(`[workspace.package] version`), `package.json`(`version`), `src-tauri/tauri.conf.json`(`version`)의 버전을 동일하게 맞춥니다.
- **서명 자격 증명 부재**: `using-credentials` 스킬을 사용하여 Windows 자격 증명 관리자에 `challa-signing_key` 및 `challa-signing_password`를 등록합니다.
- **바이너리 파일 잠금 오류**: 이미 실행 중인 Challa 인스턴스가 있다면 프로세스를 종료한 뒤 다시 실행합니다.
- **GitHub CLI(`gh`) 인증 오류**: `gh auth status`를 확인하고 필요 시 `gh auth login`을 실행합니다.

## 변경 후 검증

아이콘 원본은 `src/assets/challa-mark.svg`입니다. 사이드바와 파비콘도 이 파일을 사용합니다. 아이콘을 바꾸면 프로젝트 루트에서 `pnpm icons`를 실행해 Windows PNG와 ICO를 만든 뒤 앱을 빌드합니다. 생성 중간 파일은 `docs/icon-build/`에 보관합니다. 명령이 실패하면 원본 SVG와 Tauri CLI 출력을 확인하고 다시 실행합니다. 성공한 아이콘은 `src-tauri/icons/`에서 확인합니다.

엔진과 저장소 변경 후 `cargo test --workspace`, 화면 변경 후 `pnpm run build`를 실행합니다.

데스크톱 설정과 테마를 변경하면 격리된 미리보기에서 아래 검사를 실행합니다. `node scripts/test_desktop_ui.mjs http://127.0.0.1:4179`는 설정 저장과 재진입, 장식 영역 제거, 소형 화면과 색 대비를 확인합니다. 실제 Windows 설정과 사용자 보관함은 변경하지 않습니다. 종료 코드 0과 `docs/desktop-ui/results.json`으로 판정합니다. 브라우저 실행이 정책으로 차단되면 미실행으로 기록하고 배포용 앱의 트레이와 Windows 로그인 시작은 사용자가 확인합니다.

Bitwarden 변경 시 Rust 테스트는 별도 CLI 응답을 사용해 잠금 경쟁, 유휴 만료, 비밀값 비노출과 TOTP 경계를 검사합니다. 실제 CLI 설치 확인은 새 프로필 경로로 실행합니다. 기존 CLI 데이터 폴더는 지정하지 않습니다.

```powershell
cargo run -p challa-bitwarden --example status_probe -- ./docs/bitwarden-smoke-profile
node scripts/test_bitwarden_ui.mjs http://127.0.0.1:4179
```

첫 명령은 CLI 상태만 출력합니다. 둘째 명령은 격리된 브라우저의 모의 IPC로 인증 실패, 잠금 해제, 필드 선택, 참조 저장과 잠금을 확인합니다. 실제 계정이나 비밀번호를 사용하지 않습니다. 종료 코드 0과 `docs/bitwarden-ui/results.json`을 확인합니다. 실행 정책이 브라우저 검사를 차단한 환경에서는 미실행으로 기록하고 실제 계정 인증은 앱에서 별도로 확인합니다.

Windows 입력 동작 변경 시 아래 회귀 검사를 실행합니다. 잠금이 해제된 데스크톱이 필요하며 검사 중 테스트 창이 포커스를 사용합니다. 다른 Challa 인스턴스를 닫고 실행합니다. 검사는 예제 문구만 입력하고, 기존 클립보드를 보존하며, 다른 창으로 포커스가 바뀌면 입력을 중단합니다.

```powershell
cargo build -p challa-core --example hook_probe
uv run --python 3.13 --with pywin32 python scripts/test_windows_input.py --all
```

종료 코드 0과 `docs/native-input/`의 결과 파일을 확인합니다. 메모장 검사는 고유 이름의 테스트 문서만 사용합니다. 관리자 권한으로 실행한 입력창에는 같은 권한이 필요합니다. 디버그 검사용 입력 표식은 릴리스 빌드에서 처리하지 않습니다.

화면 변경 후 로컬 개발 서버를 실행하고 격리된 브라우저로 생성, 수정, 오류, 삭제와 반응형 흐름을 확인합니다.

```powershell
pnpm dev --port 4179
node scripts/test_ui.mjs http://127.0.0.1:4179
```

종료 코드 0과 `docs/ui-tests/results.json`을 확인합니다. 화면 검사는 브라우저의 예제 데이터만 사용합니다. 검사 실패 시 로그와 결과 이미지를 확인하고 수정한 뒤 재실행합니다.

입력 설계 대조 자료: [Beeftext InputManager.cpp](https://github.com/xmichelo/Beeftext/blob/master/Beeftext/InputManager.cpp), [BeeftextUtils.cpp](https://github.com/xmichelo/Beeftext/blob/master/Beeftext/BeeftextUtils.cpp), [XMiLib SystemUtils.cpp](https://github.com/xmichelo/XMiLib/blob/master/XMiLib/SystemUtils.cpp)

## 생산성 기능 변경 후 검증

보관함 검색과 항목 선택을 바꾸면 아래 검사를 실행합니다. Node.js 24 이상과 설치된 프로젝트 의존성이 필요합니다. 화면 검사는 합성 항목과 별도 headless 브라우저를 사용하고 포트 4183의 임시 Vite 서버를 직접 열고 닫습니다. balance UI 검사기는 현재 계정의 `~/.agents/skills/balance/scripts/ui-audit.mjs`를 사용합니다.

```powershell
node --test scripts/test_vault_search.mjs
node scripts/test_vault_picker.mjs
```

종료 코드 0, `docs/vault-picker-ui/results.json`과 `ui-audit.json`을 확인합니다. 검색, 키보드 선택, 잠금과 로딩 상태, 팝업 경계와 입력창 인접성을 검사합니다. 실제 보관함 인증과 네이티브 키 주입은 포함하지 않습니다.

IME 오류를 조사할 때 아래 읽기 전용 진단으로 현재 입력 상태 플래그를 확인할 수 있습니다. 텍스트를 읽거나 키를 보내거나 입력 모드를 바꾸지 않습니다.

```powershell
cargo run -q -p challa-core --example ime_status
```

프로필, 템플릿, 매처와 이벤트를 바꾸면 `cargo test --workspace`와 `pnpm run build`를 실행합니다. 테스트 성공과 빌드 종료 코드 0을 확인합니다. 매처 성능을 바꾸면 아래 벤치마크로 100개, 1,000개와 10,000개를 비교합니다.

```powershell
cargo run --release -p challa-core --example matcher_bench
```

출력은 일괄 구성 시간, 반복 추가 구성 시간과 성공 매칭의 평균 시간입니다. 10,000개의 반복 추가는 실행 시간을 제한하기 위해 생략합니다. 이 수치는 앱 전체 시작 시간이나 실제 입력창의 치환 지연을 뜻하지 않습니다.

빠른 호출, 템플릿 제출과 상태 이벤트를 바꾼 뒤에는 격리된 IPC 예제 검사를 실행합니다. Node.js, ego-browser와 포트 4180의 로컬 개발 서버가 필요합니다. 기존 서버 관리 상태를 확인한 뒤 실행합니다.

```powershell
pnpm dev --host 127.0.0.1 --port 4180 --strictPort
ego-browser nodejs -e "await import('file:///D:/PROJECTS/Challa/scripts/test_productivity_ego.mjs')"
```

두 번째 명령은 현재 저장소의 절대 경로에 맞춥니다. 검사는 `scripts/fixtures/productivity.js`의 예제만 사용하며 실제 키 입력과 보관함 인증을 수행하지 않습니다. 종료 코드 0, `docs/productivity-ipc-results.json`의 참 값과 `uiPollingCalls=0`을 확인합니다. 실패 시 해당 브라우저 공간과 로그를 확인한 후 재실행합니다. 기존 검증 공간을 계속 사용할 때는 호출 전에 `globalThis.challaTestSpace`와 `globalThis.challaTestPage`에 공간 ID와 페이지 라벨을 지정합니다.

네이티브 입력창, 포커스 복귀와 클립보드 경로를 변경하면 앞 절의 Windows 입력 회귀 검사를 함께 실행합니다. 데스크톱과 클립보드 접근이 허용된 세션이 필요합니다. 입력 전 접근 검사에서 실패하면 실제 입력 검증을 미실행으로 기록하고, 해당 세션의 접근 상태를 확인한 뒤 다시 실행합니다. Rust와 IPC 검사는 이 네이티브 검사를 대신하지 않습니다.

디버그 앱을 별도 데이터 폴더로 검사할 때는 절대 경로의 `CHALLA_TEST_DATA_DIR` 환경변수를 지정할 수 있습니다. 이 경우 일반 앱의 데이터와 단일 인스턴스 상태를 사용하지 않습니다. 이 설정은 릴리스 빌드에서 적용하지 않습니다.
