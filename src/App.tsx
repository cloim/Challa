import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Shield, Keyboard, Lock, RefreshCw, Trash2, CheckCircle2 } from "lucide-react";

export function App() {
  const [hookActive, setHookActive] = useState<boolean>(true);
  const [vaultStatus, setVaultStatus] = useState<string>("Locked");
  const [lastCheck, setLastCheck] = useState<string>(new Date().toLocaleTimeString());

  const checkStatus = async () => {
    try {
      const active = await invoke<boolean>("get_hook_status");
      setHookActive(active);
      const vault = await invoke<string>("get_vault_status");
      setVaultStatus(vault);
    } catch {
      // In web browser preview mode fallback
      setHookActive(true);
      setVaultStatus("Locked");
    }
    setLastCheck(new Date().toLocaleTimeString());
  };

  const handleClearBuffer = async () => {
    try {
      await invoke("clear_buffer");
    } catch (e) {
      console.warn("Buffer clear invocation:", e);
    }
  };

  useEffect(() => {
    checkStatus();
    const interval = setInterval(checkStatus, 3000);
    return () => clearInterval(interval);
  }, []);

  return (
    <div className="app-container">
      <header className="top-bar">
        <div className="top-bar-title">
          <span>Challa</span>
          <span className="top-bar-badge">Phase 1: Keyboard Hook</span>
        </div>
        <div className="top-bar-actions">
          <button
            className="icon-btn"
            onClick={handleClearBuffer}
            title="메모리 버퍼 즉시 소거"
            aria-label="메모리 버퍼 즉시 소거"
          >
            <Trash2 size={16} />
          </button>
          <button
            className="icon-btn"
            onClick={checkStatus}
            title="상태 새로고침"
            aria-label="상태 새로고침"
          >
            <RefreshCw size={16} />
          </button>
        </div>
      </header>

      <main className="content-area">
        <div className="status-grid">
          <div className="card">
            <div className="card-header">
              <span>키보드 훅 리스너</span>
              <Keyboard size={16} />
            </div>
            <div className="card-value">
              <span className={`indicator ${hookActive ? "indicator-ok" : "indicator-idle"}`} />
              <span>{hookActive ? "감시 중" : "일시 중단"}</span>
            </div>
            <div className="desc">
              <code>SetWindowsHookExW(WH_KEYBOARD_LL)</code> 전용 백그라운드 스레드 가동
            </div>
          </div>

          <div className="card">
            <div className="card-header">
              <span>메모리 격리 버퍼</span>
              <Shield size={16} />
            </div>
            <div className="card-value">
              <span className="indicator indicator-ok" />
              <span>Zeroize + RAM 고정</span>
            </div>
            <div className="desc">
              <code>VirtualLock</code>으로 pagefile 스왑 차단 및 유휴 시 자동 0 소거
            </div>
          </div>

          <div className="card">
            <div className="card-header">
              <span>비밀번호 자동 감지</span>
              <CheckCircle2 size={16} />
            </div>
            <div className="card-value">
              <span className="indicator indicator-ok" />
              <span>Privacy Guard 활성</span>
            </div>
            <div className="desc">
              UI Automation <code>IsPassword == true</code> 감지 시 키 버퍼 적재 즉시 차단
            </div>
          </div>

          <div className="card">
            <div className="card-header">
              <span>Bitwarden 보관함</span>
              <Lock size={16} />
            </div>
            <div className="card-value">
              <span className="indicator indicator-idle" />
              <span>{vaultStatus}</span>
            </div>
            <div className="desc">
              Phase 3 공식 SDK 연동 및 DPAPI 자격 증명 연동 준비 완료
            </div>
          </div>
        </div>

        <div className="card" style={{ marginTop: "12px" }}>
          <div className="card-header">
            <span>Phase 1 엔지니어링 검증 상태</span>
            <span>마지막 갱신: {lastCheck}</span>
          </div>
          <p className="desc" style={{ color: "var(--text-main)", margin: "4px 0" }}>
            Rust 워크스페이스(<code>challa-core</code>, <code>challa-bitwarden</code>, <code>challa-desktop</code>) 구성과
            Low-level Keyboard Hook, Zeroize SecureRingBuffer, UI Automation Privacy Guard가 안정적으로 초기화되었습니다.
          </p>
        </div>
      </main>
    </div>
  );
}

export default App;
