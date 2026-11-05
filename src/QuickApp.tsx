import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Search, X } from "lucide-react";
import { desktop, fieldLabel, listSnippets, type Snippet } from "./api";
import { cancelQuick, profileLabels, quickContext, submitQuick, templateFields, productivityState, readinessLabels, type QuickContext } from "./productivity";
import { TemplateForm } from "./components/TemplateForm";

export function QuickApp() {
  const [context, setContext] = useState<QuickContext | null>(null); const [rows, setRows] = useState<Snippet[]>([]);
  const [query, setQuery] = useState(""); const [selected, setSelected] = useState<Snippet | null>(null);
  const [index, setIndex] = useState(0); const [error, setError] = useState(""); const [busy, setBusy] = useState(false);
  const [readiness, setReadiness] = useState<Record<string, string>>({});
  const refresh = useCallback(async () => {
    setSelected(null); setQuery(""); setError(""); setIndex(0);
    try {
      const [context, snippets, state] = await Promise.all([desktop ? quickContext() : Promise.resolve({ token: 0, snippet_id: null, application: "미리보기", profile: "work" }), listSnippets(), productivityState()]);
      setContext(context); setRows(snippets);
      setReadiness(state.readiness);
      if (context.snippet_id) { const snippet = snippets.find(s => s.id === context.snippet_id); if (snippet) setSelected(snippet); }
      requestAnimationFrame(() => document.querySelector<HTMLInputElement>('input[aria-label="빠른 스니펫 검색"]')?.focus());
    } catch (e) { setContext(null); setError(String(e)); }
  }, []);
  useEffect(() => {
    let disposed = false; const unlisten: (() => void)[] = [];
    if (desktop) void Promise.all([listen("quick-open", () => void refresh()), listen("quick-close", () => { setContext(null); setSelected(null); setQuery(""); }), listen<{readiness: Record<string,string>}>("productivity-state", e => setReadiness(e.payload.readiness))]).then(fns => { if (disposed) fns.forEach(fn => fn()); else unlisten.push(...fns); }).catch(e => setError(String(e)));
    void refresh();
    return () => { disposed = true; unlisten.forEach(fn => fn()); };
  }, [refresh]);
  const close = () => { setSelected(null); setContext(null); if (desktop) void cancelQuick(); else location.hash = "/snippets"; };
  const matches = useMemo(() => rows.filter(s => s.enabled && ((s.profile ?? "common") === "common" || s.profile === context?.profile) && `${s.name} ${s.keyword}`.toLowerCase().includes(query.toLowerCase())), [rows, query, context]);
  const visible = useMemo(() => matches.slice(0, 100), [matches]);
  useEffect(() => { if (!selected) document.querySelector(`[data-quick-index="${index}"]`)?.scrollIntoView({ block: "nearest" }); }, [index, query, selected]);
  const insert = async (snippet: Snippet, values: Record<string, string> = {}) => {
    if (!context || busy) return;
    setBusy(true); setError("");
    try { if (desktop) await submitQuick(context.token, snippet.id, values); else setError("데스크톱 앱에서 원래 입력창에 삽입해요"); }
    catch (e) { setError(String(e)); }
    finally { setBusy(false); }
  };
  const choose = (snippet: Snippet) => {
    const reason = readiness[snippet.id] ?? ("BitwardenRef" in snippet.content ? "vault_locked" : "ready");
    if (!["ready", "template", "profile"].includes(reason)) { setError(`${readinessLabels[reason]}: 관리 화면에서 준비 상태를 확인해 주세요`); return; }
    try {
      if (snippet.template && "PlainText" in snippet.content && templateFields(snippet.content.PlainText).length) setSelected(snippet);
      else void insert(snippet);
    } catch (e) { setError(String(e)); }
  };
  return <main className="flex h-dvh flex-col bg-panel text-ink" onKeyDown={e => {
    if (e.key === "Escape" && !busy) { e.preventDefault(); close(); }
    if (!selected && e.key === "ArrowDown") { e.preventDefault(); setIndex(i => Math.min(i + 1, visible.length - 1)); }
    if (!selected && e.key === "ArrowUp") { e.preventDefault(); setIndex(i => Math.max(0, i - 1)); }
    if (!selected && e.key === "Enter" && visible[index] && !busy) { e.preventDefault(); choose(visible[index]); }
  }}>
    <header className="flex shrink-0 items-center justify-between border-b border-line px-5 py-4"><div><h1 className="text-sm font-semibold">{selected ? selected.name : "빠른 호출"}</h1><p className="mt-1 text-xs text-muted">{context ? `${context.application} | ${profileLabels[context.profile]} 프로필` : "Ctrl+Alt+Space로 입력할 앱에서 실행하세요"}</p></div><button aria-label="빠른 호출 닫기" className="icon-button" onClick={close} disabled={busy}><X size={15} /></button></header>
    {selected && "PlainText" in selected.content ? <div className="min-h-0 flex-1 overflow-y-auto p-5"><TemplateForm key={`${context?.token}-${selected.id}`} text={selected.content.PlainText} onCancel={close} onSubmit={values => insert(selected, values)} /></div> : <>
      <label className="mx-5 my-4 flex shrink-0 items-center gap-2 rounded-lg border border-line px-3"><Search size={15} /><input aria-label="빠른 스니펫 검색" className="h-10 min-w-0 flex-1 bg-transparent text-xs outline-none" value={query} onChange={e => { setQuery(e.target.value); setIndex(0); }} placeholder="이름 또는 단축어 검색" /></label>
      <div className="min-h-0 flex-1 overflow-y-auto px-3" role="listbox" aria-label="빠른 호출 스니펫">{visible.map((s, i) => <button key={s.id} data-quick-index={i} role="option" aria-selected={index === i} disabled={busy || !context} className={`flex w-full items-center gap-3 rounded-lg px-3 py-3 text-left text-xs ${index === i ? "bg-selected" : "hover:bg-canvas"}`} onMouseEnter={() => setIndex(i)} onClick={() => choose(s)}><div className="min-w-0 flex-1"><span className="block truncate">{s.name}</span><span className="mt-1 block truncate text-xs text-muted">{"PlainText" in s.content ? s.content.PlainText : `Bitwarden | ${fieldLabel(s.content.BitwardenRef.field)} | ${readinessLabels[readiness[s.id] ?? "vault_locked"]}`}</span></div><code className="keyword-chip">{s.keyword}</code></button>)}{!visible.length && <p className="py-10 text-center text-xs text-muted">사용할 수 있는 스니펫이 없어요</p>}</div>
    </>}
    {error && <p role="alert" className="shrink-0 px-5 py-3 text-xs text-rose-300">{error}</p>}
    {matches.length > 100 && <footer className="shrink-0 border-t border-line px-5 py-3 text-[12px] text-muted">100 / {matches.length}</footer>}
  </main>;
}
