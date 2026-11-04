import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Shield,
  Keyboard,
  Lock,
  RefreshCw,
  Trash2,
  Plus,
  Zap,
  FileText,
} from "lucide-react";

interface Snippet {
  id: string;
  name: string;
  keyword: string;
  content: { PlainText: string } | { BitwardenRef: { item_id: string; field: string } };
  trigger_mode: "Immediate" | "Space" | "Enter" | "Tab" | "Delimiter";
  enabled: boolean;
}

export function App() {
  const [hookActive, setHookActive] = useState<boolean>(true);
  const [vaultStatus, setVaultStatus] = useState<string>("Locked");
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [newKeyword, setNewKeyword] = useState<string>("");
  const [newName, setNewName] = useState<string>("");
  const [newContent, setNewContent] = useState<string>("");
  const [newTrigger, setNewTrigger] = useState<"Immediate" | "Space">("Immediate");
  const [testText, setTestText] = useState<string>("");
  const [lastCheck, setLastCheck] = useState<string>(new Date().toLocaleTimeString());

  const checkStatus = async () => {
    try {
      const active = await invoke<boolean>("get_hook_status");
      setHookActive(active);
      const vault = await invoke<string>("get_vault_status");
      setVaultStatus(vault);
      const list = await invoke<Snippet[]>("list_snippets");
      setSnippets(list);
    } catch {
      // Fallback for browser preview
      setHookActive(true);
      setVaultStatus("Locked");
      setSnippets([
        {
          id: "welcome_1",
          name: "Challa 소개",
          keyword: ":challa",
          content: { PlainText: "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더" },
          trigger_mode: "Immediate",
          enabled: true,
        },
        {
          id: "welcome_2",
          name: "서명 템플릿",
          keyword: ":sig",
          content: { PlainText: "감사합니다.\nChalla 드림" },
          trigger_mode: "Space",
          enabled: true,
        },
      ]);
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

  const handleAddSnippet = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newKeyword.trim() || !newContent.trim()) return;

    const snippet: Snippet = {
      id: `snip_${Date.now()}`,
      name: newName.trim() || newKeyword.trim(),
      keyword: newKeyword.trim(),
      content: { PlainText: newContent },
      trigger_mode: newTrigger,
      enabled: true,
    };

    try {
      await invoke("add_snippet", { snippet });
      setNewKeyword("");
      setNewName("");
      setNewContent("");
      await checkStatus();
    } catch {
      setSnippets((prev) => [...prev, snippet]);
      setNewKeyword("");
      setNewName("");
      setNewContent("");
    }
  };

  const handleRemoveSnippet = async (id: string) => {
    try {
      await invoke("remove_snippet", { id });
      await checkStatus();
    } catch {
      setSnippets((prev) => prev.filter((s) => s.id !== id));
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
          <span className="top-bar-badge">Phase 2: Snippet Engine</span>
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
              <span>키보드 훅 & 인젝터</span>
              <Keyboard size={16} />
            </div>
            <div className="card-value">
              <span className={`indicator ${hookActive ? "indicator-ok" : "indicator-idle"}`} />
              <span>{hookActive ? "가동 중" : "대기"}</span>
            </div>
            <div className="desc">
              <code>SendInput(UNICODE)</code> 직접 타이핑 및 Windows 클립보드 격리 주입
            </div>
          </div>

          <div className="card">
            <div className="card-header">
              <span>스니펫 매처</span>
              <Zap size={16} />
            </div>
            <div className="card-value">
              <span className="indicator indicator-ok" />
              <span>{snippets.length}개 등록됨</span>
            </div>
            <div className="desc">
              Reverse Trie 기반 Suffix 즉시 치환 및 스페이스/엔터 트리거
            </div>
          </div>

          <div className="card">
            <div className="card-header">
              <span>메모리 보호 & 프라이버시</span>
              <Shield size={16} />
            </div>
            <div className="card-value">
              <span className="indicator indicator-ok" />
              <span>VirtualLock + UI Guard</span>
            </div>
            <div className="desc">
              비밀번호 필드 감지 시 버퍼 차단 및 클립보드 이력(Win+V) 유출 방지
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
              Phase 3 SDK 연동 및 DPAPI 자격 증명 연계 대기
            </div>
          </div>
        </div>

        {/* Live Typing Playground */}
        <div className="card">
          <div className="card-header">
            <span>스니펫 실시간 타이핑 테스트 공간</span>
            <span>마지막 동기화: {lastCheck}</span>
          </div>
          <div className="desc" style={{ marginBottom: "8px" }}>
            아래 입력창에서 등록된 단축어(예: <code>:challa</code> 또는 <code>:sig</code> + 스페이스)를 직접 입력해 보세요.
          </div>
          <textarea
            style={{
              width: "100%",
              boxSizing: "border-box",
              minHeight: "70px",
              padding: "10px",
              borderRadius: "6px",
              backgroundColor: "var(--bg-primary)",
              color: "var(--text-main)",
              border: "1px solid var(--border)",
              fontFamily: "monospace",
              fontSize: "14px",
              resize: "vertical",
            }}
            placeholder="여기에 :challa 또는 :sig 를 입력해 보세요..."
            value={testText}
            onChange={(e) => setTestText(e.target.value)}
          />
        </div>

        {/* Registered Snippets List */}
        <div className="card">
          <div className="card-header">
            <span>등록된 스니펫 목록</span>
            <FileText size={16} />
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: "8px", marginTop: "8px" }}>
            {snippets.map((snip) => (
              <div
                key={snip.id}
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                  padding: "10px 14px",
                  borderRadius: "6px",
                  backgroundColor: "var(--bg-card)",
                }}
              >
                <div style={{ display: "flex", alignItems: "center", gap: "12px" }}>
                  <code
                    style={{
                      backgroundColor: "rgba(56, 189, 248, 0.15)",
                      color: "var(--primary)",
                      padding: "3px 8px",
                      borderRadius: "4px",
                      fontWeight: 700,
                    }}
                  >
                    {snip.keyword}
                  </code>
                  <div>
                    <div style={{ fontWeight: 600, fontSize: "14px" }}>{snip.name}</div>
                    <div style={{ fontSize: "12px", color: "var(--text-muted)" }}>
                      {"PlainText" in snip.content
                        ? snip.content.PlainText.replace(/\n/g, " ↵ ")
                        : "Bitwarden 보관함 참조"}
                    </div>
                  </div>
                </div>

                <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                  <span
                    style={{
                      fontSize: "11px",
                      padding: "2px 8px",
                      borderRadius: "9999px",
                      backgroundColor: "rgba(52, 211, 153, 0.15)",
                      color: "var(--success)",
                    }}
                  >
                    {snip.trigger_mode === "Immediate" ? "즉시 치환" : "스페이스바"}
                  </span>
                  <button
                    className="icon-btn"
                    onClick={() => handleRemoveSnippet(snip.id)}
                    title="스니펫 삭제"
                    aria-label="스니펫 삭제"
                  >
                    <Trash2 size={14} />
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Add New Snippet Form */}
        <div className="card">
          <div className="card-header">
            <span>새 스니펫 등록</span>
            <Plus size={16} />
          </div>
          <form
            onSubmit={handleAddSnippet}
            style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: "10px", marginTop: "8px" }}
          >
            <input
              type="text"
              placeholder="단축어 (예: :email)"
              value={newKeyword}
              onChange={(e) => setNewKeyword(e.target.value)}
              style={{
                padding: "8px 12px",
                borderRadius: "6px",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-main)",
                border: "1px solid var(--border)",
              }}
              required
            />
            <input
              type="text"
              placeholder="스니펫 이름 (예: 개인 이메일)"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              style={{
                padding: "8px 12px",
                borderRadius: "6px",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-main)",
                border: "1px solid var(--border)",
              }}
            />
            <textarea
              placeholder="치환될 본문 텍스트"
              value={newContent}
              onChange={(e) => setNewContent(e.target.value)}
              style={{
                gridColumn: "1 / -1",
                minHeight: "60px",
                padding: "8px 12px",
                borderRadius: "6px",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-main)",
                border: "1px solid var(--border)",
              }}
              required
            />
            <div style={{ display: "flex", alignItems: "center", gap: "10px" }}>
              <label style={{ fontSize: "13px", color: "var(--text-muted)" }}>트리거 방식:</label>
              <select
                value={newTrigger}
                onChange={(e) => setNewTrigger(e.target.value as "Immediate" | "Space")}
                style={{
                  padding: "6px 10px",
                  borderRadius: "6px",
                  backgroundColor: "var(--bg-primary)",
                  color: "var(--text-main)",
                  border: "1px solid var(--border)",
                }}
              >
                <option value="Immediate">즉시 치환</option>
                <option value="Space">스페이스바 후 치환</option>
              </select>
            </div>
            <div style={{ display: "flex", justifyContent: "flex-end" }}>
              <button
                type="submit"
                style={{
                  padding: "8px 16px",
                  borderRadius: "6px",
                  backgroundColor: "var(--primary)",
                  color: "#0f172a",
                  border: "none",
                  fontWeight: 700,
                  cursor: "pointer",
                }}
              >
                스니펫 추가
              </button>
            </div>
          </form>
        </div>
      </main>
    </div>
  );
}

export default App;
