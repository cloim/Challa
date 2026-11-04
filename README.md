# Challa (찰라)

> Beeftext의 핵심 기능을 계승하며, 평문과 Bitwarden 보관함 값을 안전하게 주입하는 고보안 Rust & Tauri v2 기반 텍스트 익스팬더.

## 주요 특징
1. **스니펫 지원**: 평문 텍스트 및 Bitwarden 보관함(Password, Username, TOTP, Custom Field) 실시간 조회 및 주입
2. **보안 우선 설계**:
   - `windows-rs` Low-level Keyboard Hook 사용
   - 키 입력 버퍼의 메모리 격리 (`zeroize`, `VirtualLock`)
   - Windows UI Automation 기반 비밀번호 입력창 자동 감지 시 버퍼 적재 생략
   - 소량 텍스트 `SendInput(KEYEVENTF_UNICODE)` 직접 타이핑 (클립보드 미사용)
   - 장문 클립보드 주입 시 `CanIncludeInClipboardHistory = 0` 및 `ExcludeClipboardContentFromMonitorProcessing` 적용 후 즉시 원복
   - 공식 `bitwarden-sdk` (Rust) 기반 인메모리 복호화 및 세션 자동 잠금
   - DPAPI 기반 자격 증명 안전 관리
3. **데스크톱 UI**: Tauri v2 기반 경량 시스템 트레이 상주 및 관리 모달

## 상세 설계 문서
- [docs/plan.md](docs/plan.md) 참고
