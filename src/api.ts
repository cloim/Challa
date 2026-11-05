import { invoke, isTauri } from "@tauri-apps/api/core";
export type Trigger = "Immediate" | "Space" | "Enter" | "Tab" | "Delimiter";
export type VaultField = "Username" | "Password" | "Totp" | "Notes" | { Custom: string };
export interface VaultReference { item_id: string; field: VaultField }
export interface Snippet { id: string; name: string; keyword: string; content: { PlainText: string } | { BitwardenRef: VaultReference }; trigger_mode: Trigger; enabled: boolean; profile?: string; template?: boolean }
export interface VaultInfo { status: "Unavailable" | "Unauthenticated" | "Locked" | "Unlocked"; cli_available: boolean; cached_fields: number; auto_lock_minutes: number }
export interface VaultItem { id: string; name: string; username: string | null; fields: VaultField[] }
export const fieldLabel = (field: VaultField) => typeof field === "object" ? field.Custom : ({ Username: "사용자 이름", Password: "비밀번호", Totp: "일회용 코드 (TOTP)", Notes: "보안 메모" })[field];
export const vaultLabels: Record<VaultInfo["status"], string> = { Unavailable: "CLI 설치 필요", Unauthenticated: "로그인 필요", Locked: "잠김", Unlocked: "잠금 해제됨" };
export interface EngineStatus { active: boolean; paused: boolean; vault: string; error: string | null }
export const desktop = isTauri();
export const triggerLabels: Record<Trigger, string> = { Immediate: "즉시 치환", Space: "스페이스", Enter: "Enter", Tab: "Tab", Delimiter: "구분 문자" };
const samples: Snippet[] = [
  { id: "welcome_1", name: "Challa 소개", keyword: ":challa", content: { PlainText: "Challa: Rust & Tauri v2 기반 고보안 텍스트 익스팬더" }, trigger_mode: "Immediate", enabled: true },
  { id: "welcome_2", name: "서명 템플릿", keyword: ":sig", content: { PlainText: "감사합니다.\nChalla 드림" }, trigger_mode: "Space", enabled: true },
];
function previewSnippets(): Snippet[] { const saved = localStorage.getItem("challa-preview-snippets"); return saved ? JSON.parse(saved) : samples; }
export async function listSnippets(): Promise<Snippet[]> { return desktop ? invoke("list_snippets") : previewSnippets(); }
export async function engineStatus(): Promise<EngineStatus> { return desktop ? invoke("get_engine_status") : { active: false, paused: false, vault: "Locked", error: null }; }
export async function saveSnippet(snippet: Snippet): Promise<void> {
  if (desktop) return invoke("add_snippet", { snippet });
  const rows = previewSnippets();
  if (rows.some(s => s.id !== snippet.id && s.keyword === snippet.keyword && s.trigger_mode === snippet.trigger_mode && ((s.profile ?? "common") === (snippet.profile ?? "common") || (s.profile ?? "common") === "common" || (snippet.profile ?? "common") === "common"))) throw new Error("같은 단축어와 실행 방식이 이미 등록되어 있어요");
  localStorage.setItem("challa-preview-snippets", JSON.stringify([...rows.filter(s => s.id !== snippet.id), snippet]));
}
export async function deleteSnippet(id: string): Promise<void> {
  if (desktop) { await invoke("remove_snippet", { id }); return; }
  localStorage.setItem("challa-preview-snippets", JSON.stringify(previewSnippets().filter(s => s.id !== id)));
}
export async function pauseEngine(paused: boolean): Promise<void> { if (desktop) await invoke("set_engine_paused", { paused }); }
export async function clearBuffer(): Promise<void> { if (desktop) await invoke("clear_buffer"); }
const previewVault: VaultInfo = { status: "Unauthenticated", cli_available: false, cached_fields: 0, auto_lock_minutes: 15 };
export async function vaultInfo(inspect = false): Promise<VaultInfo> { return desktop ? invoke(inspect ? "inspect_vault" : "get_vault_status") : previewVault; }
export async function loginVault(server: string, clientId: string, clientSecret: string): Promise<VaultInfo> { return invoke("login_vault", { server, clientId, clientSecret }); }
export async function unlockVault(password: string): Promise<VaultInfo> { return invoke("unlock_vault", { password }); }
export async function lockVault(): Promise<void> { if (desktop) await invoke("lock_vault"); }
export async function logoutVault(): Promise<VaultInfo> { return invoke("logout_vault"); }
export async function syncVault(): Promise<VaultInfo> { return invoke("sync_vault"); }
export async function vaultItems(): Promise<VaultItem[]> { return desktop ? invoke("list_vault_items") : []; }
