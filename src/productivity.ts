import { invoke } from "@tauri-apps/api/core";
import { desktop, type Snippet, type Trigger } from "./api";

export const profileLabels: Record<string, string> = { common: "공통", work: "업무", personal: "개인" };
export const readinessLabels: Record<string, string> = { ready: "사용 가능", template: "빈칸 입력", disabled: "사용 중지", paused: "엔진 일시 정지", engine_error: "엔진 확인 필요", profile: "다른 프로필", vault_locked: "보관함 잠김", field_missing: "필드 준비 필요" };
export interface ProfileSettings { active_profile: string; rules: { application: string; profile: string }[] }
export interface ProductivityState { settings: ProfileSettings; effective_profile: string; application: string; readiness: Record<string, string>; shortcut_error?: string | null }
export interface QuickContext { token: number; snippet_id: string | null; application: string; profile: string }
const defaultSettings: ProfileSettings = { active_profile: "work", rules: [] };
export async function productivityState(): Promise<ProductivityState> {
  if (desktop) return invoke("get_productivity_state");
  const settings = JSON.parse(localStorage.getItem("challa-preview-profiles") ?? "null") ?? defaultSettings;
  return { settings, effective_profile: settings.active_profile, application: "브라우저 미리보기", readiness: {} };
}
export async function saveProfiles(settings: ProfileSettings) {
  if (desktop) return invoke("save_profiles", { settings });
  if (!settings.rules.every((r, i) => /^[^\\/:*?]+\.exe$/i.test(r.application) && settings.rules.findIndex(s => s.application.toLowerCase() === r.application.toLowerCase()) === i)) throw new Error("중복 없이 앱 실행 파일 이름을 입력해 주세요");
  localStorage.setItem("challa-preview-profiles", JSON.stringify(settings));
}
function fires(mode: Trigger, last: string | undefined) {
  return mode === "Immediate" || (mode === "Space" && last === " ") || (mode === "Tab" && last === "\t") || (mode === "Enter" && ["\n", "\r"].includes(last ?? "")) || (mode === "Delimiter" && /[ \t\r\n,.;:!?/\-]/.test(last ?? ""));
}
function matches(word: string, s: Snippet) {
  return s.trigger_mode === "Immediate" ? word.endsWith(s.keyword) : word.slice(0, -1).endsWith(s.keyword) && fires(s.trigger_mode, word.at(-1));
}
export function snippetConflicts(draft: Snippet, rows: Snippet[]): string[] {
  const messages = new Set<string>();
  for (const other of rows.filter(s => s.id !== draft.id && ((s.profile ?? "common") === (draft.profile ?? "common") || (s.profile ?? "common") === "common" || (draft.profile ?? "common") === "common"))) {
    if (other.keyword === draft.keyword && (other.trigger_mode === draft.trigger_mode || other.trigger_mode === "Delimiter" || draft.trigger_mode === "Delimiter")) { messages.add(`「${other.name}」와 단축어 및 실행 조건이 겹쳐요`); continue; }
    if (!other.enabled || !draft.enabled) continue;
    for (const [first, second] of [[other, draft], [draft, other]]) {
      if (!first.keyword) continue;
      for (let i = 1; i <= second.keyword.length; i++) if (matches(second.keyword.slice(0, i), first)) { messages.add(`「${second.name}」 입력 중 「${first.name}」가 먼저 실행될 수 있어요`); break; }
    }
  }
  return [...messages].sort();
}
export async function diagnoseSnippet(snippet: Snippet, rows: Snippet[]) { return desktop ? invoke<string[]>("diagnose_snippet", { snippet }) : snippetConflicts(snippet, rows); }
export function templateFields(text: string): string[] {
  const fields: string[] = [];
  let rest = text;
  while (rest.includes("{{")) {
    const start = rest.indexOf("{{");
    if (rest.slice(0, start).includes("}}")) throw new Error("템플릿 괄호를 확인해 주세요");
    rest = rest.slice(start + 2); const end = rest.indexOf("}}");
    if (end < 0) throw new Error("템플릿의 닫는 괄호가 필요해요");
    const name = rest.slice(0, end).trim();
    if (!name || [...name].length > 40 || /[{}\r\n]/.test(name)) throw new Error("빈칸 이름은 괄호와 줄바꿈 없이 1~40자로 입력해 주세요");
    if (!fields.includes(name)) fields.push(name);
    if (fields.length > 32) throw new Error("빈칸은 최대 32개까지 사용할 수 있어요");
    rest = rest.slice(end + 2);
  }
  if (rest.includes("}}")) throw new Error("템플릿 괄호를 확인해 주세요");
  return fields;
}
export function renderTemplate(text: string, values: Record<string, string>) {
  const fields = templateFields(text);
  if (fields.some(f => !(f in values))) throw new Error("모든 빈칸을 입력해 주세요");
  const result = text.replace(/\{\{([^{}]+)\}\}/g, (_, name: string) => values[name.trim()]);
  if (result.length > 16384 || result.includes("\0")) throw new Error("완성 문구는 16,384자 이내로 입력해 주세요");
  return result;
}
export const quickContext = (): Promise<QuickContext> => invoke("get_quick_context");
export const cancelQuick = () => invoke("cancel_quick");
export const submitQuick = (token: number, snippetId: string, values: Record<string, string>) => invoke("submit_quick", { token, snippetId, values });
