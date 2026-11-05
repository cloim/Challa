import { listen } from "@tauri-apps/api/event";
import { ProfileSettings } from "./components/ProfileSettings";
import { TemplateForm } from "./components/TemplateForm";
import { Modal } from "./components/modal/Modal.jsx";
import { diagnoseSnippet, productivityState, profileLabels, readinessLabels, templateFields, type ProductivityState } from "./productivity";
import { useCallback, useEffect, useMemo, useRef, useState, type FormEvent, type ReactNode } from "react";
import { Check, FileText, Settings, Keyboard, LayoutGrid, LockKeyhole, Pause, Play, Plus, RefreshCw, Search, ShieldCheck, Sparkles, Trash2, X } from "lucide-react";
import type { ColDef, ICellRendererParams } from "ag-grid-community";
import { DataGrid } from "./components/DataGrid";
import { SelectControl } from "./components/SelectControl";
import { DesktopSettings } from "./components/DesktopSettings";
import { VaultPanel, VaultReferenceEditor } from "./components/VaultPanel";
import challaMark from "./assets/challa-mark.svg";
import { clearBuffer, deleteSnippet, desktop, engineStatus, fieldLabel, listSnippets, pauseEngine, saveSnippet, syncVault, triggerLabels, type EngineStatus, type Snippet, type Trigger, type VaultField, type VaultItem } from "./api";

const initialStatus: EngineStatus = { active: false, paused: false, vault: "Locked", error: null };
function IconButton({ label, children, onClick, disabled = false }: { label: string; children: ReactNode; onClick: () => void; disabled?: boolean }) {
  return <button className="icon-button" aria-label={label} data-tip={label} onClick={onClick} disabled={disabled}>{children}</button>;
}
const bodyOf = (snippet: Snippet) => "PlainText" in snippet.content ? snippet.content.PlainText : `Bitwarden | ${fieldLabel(snippet.content.BitwardenRef.field)}`;

export function App() {
  const [product, setProduct] = useState<ProductivityState>({ settings: { active_profile: "work", rules: [] }, effective_profile: "work", application: "", readiness: {} });
  const [profilesOpen, setProfilesOpen] = useState(false);
  const [templateOpen, setTemplateOpen] = useState(false);
  const [warnings, setWarnings] = useState<string[]>([]);
  const [templateError, setTemplateError] = useState("");
  const [page, setPage] = useState(location.hash === "#/vault" ? "vault" : location.hash === "#/settings" ? "settings" : "library");
  const navigate = (next: "library" | "vault" | "settings") => { location.hash = next === "vault" ? "/vault" : next === "settings" ? "/settings" : "/snippets"; setPage(next); };
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [status, setStatus] = useState(initialStatus);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<"all" | "active" | "disabled">("all");
  const [draft, setDraft] = useState<Snippet | null>(null);
  const [original, setOriginal] = useState("");
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState("");
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [toast, setToast] = useState("");
  const searchRef = useRef<HTMLInputElement>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const dirty = !!draft && JSON.stringify(draft) !== original;
  const enabledCount = snippets.filter(s => s.enabled).length;

  const refresh = useCallback(async () => {
    setError("");
    try {
      const [rows, engine, productivity] = await Promise.all([listSnippets(), engineStatus(), productivityState()]);
      setSnippets(rows); setStatus(engine); setProduct(productivity);
    } catch (error) { setError(String(error)); }
    finally { setLoading(false); }
  }, []);
  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => { const change = () => setPage(location.hash === "#/vault" ? "vault" : location.hash === "#/settings" ? "settings" : "library"); window.addEventListener("hashchange", change); return () => window.removeEventListener("hashchange", change); }, []);
  useEffect(() => {
    let disposed = false; const off: (() => void)[] = [];
    const subscribe = async () => {
      const listeners = await Promise.all([
        listen<EngineStatus | null>("engine-state", e => { if (e.payload) setStatus(e.payload); }),
        listen<ProductivityState>("productivity-state", e => setProduct(e.payload)),
        listen("library-changed", () => void refresh()),
        listen<string>("operation-error", e => setToast(e.payload)),
        listen("tauri://focus", () => void refresh()),
      ]);
      if (disposed) listeners.forEach(fn => fn()); else off.push(...listeners);
    };
    if (desktop) void subscribe().catch(e => setError(String(e)));
    const visible = () => { if (!document.hidden) void refresh(); };
    document.addEventListener("visibilitychange", visible);
    return () => { disposed = true; off.forEach(fn => fn()); document.removeEventListener("visibilitychange", visible); };
  }, [refresh]);
  useEffect(() => {
    if (!draft) { setWarnings([]); setTemplateError(""); return; }
    try { if (draft.template && "PlainText" in draft.content) templateFields(draft.content.PlainText); setTemplateError(""); }
    catch (e) { setTemplateError(e instanceof Error ? e.message : String(e)); }
    let disposed = false;
    const timer = setTimeout(() => { void diagnoseSnippet(draft, snippets).then(w => { if (!disposed) setWarnings(w); }).catch(e => { if (!disposed) setWarnings([String(e)]); }); }, 150);
    return () => { disposed = true; clearTimeout(timer); };
  }, [draft, snippets]);
  useEffect(() => { if (!toast) return; const timer = setTimeout(() => setToast(""), 3500); return () => clearTimeout(timer); }, [toast]);
  useEffect(() => { if (status.error) setToast(status.error); }, [status.error]);
  useEffect(() => { if (product.shortcut_error) setToast(product.shortcut_error); }, [product.shortcut_error]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") { event.preventDefault(); searchRef.current?.focus(); }
    };
    window.addEventListener("keydown", key); return () => window.removeEventListener("keydown", key);
  }, []);

  const open = useCallback((snippet: Snippet) => {
    if (dirty && draft?.id !== snippet.id) { setToast("작성 중인 내용을 저장하거나 취소해 주세요"); return; }
    if (draft?.id === snippet.id) return;
    setDraft(structuredClone(snippet)); setOriginal(JSON.stringify(snippet)); setFormError(""); setConfirmDelete(false);
  }, [dirty, draft?.id]);
  const create = () => {
    if (dirty) { setToast("작성 중인 내용을 먼저 저장하거나 취소해 주세요"); return; }
    setDraft({ id: crypto.randomUUID(), name: "", keyword: "", content: { PlainText: "" }, trigger_mode: "Immediate", enabled: true, profile: "common" });
    setOriginal(""); setFormError(""); setConfirmDelete(false);
    requestAnimationFrame(() => nameRef.current?.focus());
  };
  const useVaultItem = (item: VaultItem, field: VaultField) => {
    if (dirty) { setToast("작성 중인 내용을 먼저 저장하거나 취소해 주세요"); return; }
    setDraft({ id: crypto.randomUUID(), name: item.name, keyword: "", content: { BitwardenRef: { item_id: item.id, field } }, trigger_mode: "Space", enabled: true, profile: "common" });
    setOriginal(""); setFormError(""); setConfirmDelete(false); navigate("library");
  };
  const visible = useMemo(() => snippets.filter(s => (filter === "all" || s.enabled === (filter === "active")) && `${s.name} ${s.keyword} ${bodyOf(s)}`.toLocaleLowerCase().includes(query.toLocaleLowerCase())), [snippets, filter, query]);
  const readyOf = useCallback((s: Snippet) => product.readiness[s.id] ?? (!s.enabled ? "disabled" : (s.profile ?? "common") !== "common" && s.profile !== product.effective_profile ? "profile" : "BitwardenRef" in s.content ? "vault_locked" : s.template && s.content.PlainText.includes("{{") ? "template" : "ready"), [product]);
  const resolveReadiness = (reason: string) => {
    if (reason === "vault_locked") navigate("vault");
    else if (reason === "field_missing") void syncVault().then(() => refresh()).catch(e => setToast(String(e)));
    else if (reason === "profile") setProfilesOpen(true);
    else if (reason === "paused") void toggleEngine();
    else if (reason === "engine_error") void refresh();
  };
  const columns = useMemo<ColDef<Snippet>[]>(() => [
    { field: "name", headerName: "스니펫", flex: 1.5, minWidth: 190, cellRenderer: ({ data }: ICellRendererParams<Snippet>) => data && <div className="flex h-full items-center gap-3"><span className="snippet-icon"><FileText size={17} /></span><div className="min-w-0"><span className="block truncate font-medium text-ink">{data.name}</span><span className="block truncate text-xs text-muted">{bodyOf(data).replace(/\r?\n/g, " ↵ ")}</span></div></div> },
    { field: "keyword", headerName: "단축어", width: 135, minWidth: 110, cellRenderer: ({ value }: ICellRendererParams) => <code className="keyword-chip">{value}</code> },
    { field: "trigger_mode", headerName: "실행 방식", width: 120, minWidth: 110, valueFormatter: ({ value }) => triggerLabels[value as Trigger] },
    { field: "enabled", headerName: "준비 상태", width: 145, minWidth: 130, cellRenderer: ({ data }: ICellRendererParams<Snippet>) => data && <button className={`inline-flex items-center gap-2 text-xs ${["ready", "template"].includes(readyOf(data)) ? "text-mint" : "text-muted"}`} aria-label={`${data.name}: ${readinessLabels[readyOf(data)]}`} onClick={e => { e.stopPropagation(); resolveReadiness(readyOf(data)); }}><span className="size-1.5 rounded-full bg-current" />{readinessLabels[readyOf(data)]}</button> },
  ], [readyOf]);

  const save = async (event: FormEvent) => {
    event.preventDefault(); if (!draft) return;
    if (!/^[\x21-\x7e]{1,64}$/.test(draft.keyword)) { setFormError("단축어는 공백 없이 영문, 숫자와 기호 1~64자로 입력해 주세요"); return; }
    if (!draft.name.trim() || !bodyOf(draft).length || bodyOf(draft).length > 16384) { setFormError("이름과 16,384자 이내의 본문을 입력해 주세요"); return; }
    if ("BitwardenRef" in draft.content && !draft.content.BitwardenRef.item_id) { setFormError("Bitwarden 항목과 필드를 선택해 주세요"); return; }
    if (templateError) { setFormError(templateError); return; }
    setSaving(true); setFormError("");
    try {
      const saved = { ...draft, name: draft.name.trim() };
      await saveSnippet(saved); setDraft(saved); setOriginal(JSON.stringify(saved));
      await refresh(); setToast(desktop ? "스니펫을 암호화해 저장했어요" : "이 브라우저에 미리보기를 저장했어요");
    } catch (error) { setFormError(error instanceof Error ? error.message : String(error)); }
    finally { setSaving(false); }
  };
  const remove = async () => {
    if (!draft) return;
    setSaving(true);
    try { await deleteSnippet(draft.id); setDraft(null); setOriginal(""); setConfirmDelete(false); await refresh(); setToast("스니펫을 삭제했어요"); }
    catch (error) { setFormError(String(error)); }
    finally { setSaving(false); }
  };
  const toggleEngine = async () => {
    try { await pauseEngine(!status.paused); setStatus(await engineStatus()); }
    catch (error) { setToast(String(error)); }
  };

  return <div className="flex h-dvh min-h-0 flex-col bg-canvas text-ink">
    <div className="flex min-h-0 flex-1">
      <aside className="flex min-h-0 w-[210px] shrink-0 flex-col overflow-y-auto border-r border-line bg-sidebar px-4 pb-4 pt-7 max-[900px]:w-[76px] max-[900px]:px-3 max-[600px]:hidden">
        <button className="mb-8 flex items-center gap-3 px-2 text-left" aria-label="전체 스니펫으로 이동" onClick={() => { setFilter("all"); setQuery(""); navigate("library"); }}>
          <img className="brand-mark" src={challaMark} alt="" aria-hidden="true" /><span className="brand-word max-[900px]:hidden">challa<span className="text-mint">.</span></span>
        </button>
        <p className="mb-3 px-3 text-[12px] font-semibold tracking-[.12em] text-muted max-[900px]:hidden">라이브러리</p>
        <nav className="flex flex-col gap-1" aria-label="스니펫 필터">
          {([ ["all", "모든 스니펫", LayoutGrid, snippets.length], ["active", "사용 중", Sparkles, enabledCount], ["disabled", "사용 중지", Pause, snippets.length - enabledCount] ] as const).map(([value, label, Icon, count]) => <button key={value} className={`nav-item ${page === "library" && filter === value ? "active" : ""}`} onClick={() => { setFilter(value); navigate("library"); }} aria-label={label} aria-current={page === "library" && filter === value ? "page" : undefined}><Icon size={16} /><span className="flex-1 max-[900px]:hidden">{label}</span><span className="nav-count max-[900px]:hidden">{count}</span></button>)}
        </nav>
        <div className="mt-8 border-t border-line pt-5"><div className="mb-3 flex items-center justify-between px-3 text-[12px] font-semibold tracking-[.12em] text-muted max-[900px]:hidden"><span>보관함</span><LockKeyhole size={11} /></div><button className={`nav-item w-full ${page === "vault" ? "active" : ""}`} aria-label="Bitwarden 보관함 열기" aria-current={page === "vault" ? "page" : undefined} onClick={() => navigate("vault")}><ShieldCheck size={16} /><span className="max-[900px]:hidden">Bitwarden</span><span className={`ml-auto rounded border border-line px-1.5 py-0.5 text-[11px] max-[900px]:hidden ${status.vault === "Unlocked" ? "text-mint" : ""}`}>{status.vault === "Unlocked" ? "열림" : "잠김"}</span></button></div>
        <div className="mt-auto pt-6">
          <button className={`nav-item w-full ${page === "settings" ? "active" : ""}`} onClick={() => navigate("settings")} aria-label="앱 설정" aria-current={page === "settings" ? "page" : undefined}><Settings size={16} /><span className="max-[900px]:hidden">설정</span></button>
          <button className="nav-item w-full" onClick={() => setProfilesOpen(true)} aria-label="작업 프로필"><Keyboard size={16} /><span className="max-[900px]:hidden">{profileLabels[product.effective_profile]} 프로필</span></button>
        </div>
      </aside>

      {page === "settings" ? <DesktopSettings /> : page === "vault" ? <VaultPanel onUse={useVaultItem} onStatusChange={() => { void engineStatus().then(setStatus).catch(e => setToast(String(e))); }} /> : <main className="flex min-h-0 min-w-0 flex-1 flex-col">
        <header className="flex h-[76px] shrink-0 items-center justify-between gap-3 border-b border-line px-8 max-[600px]:h-[76px] max-[600px]:px-4">
          <div className="min-w-0"><h1 className="text-xl font-semibold tracking-[-.04em]" data-ui-audit-context-title>{filter === "active" ? "사용 중" : filter === "disabled" ? "사용 중지" : "모든 스니펫"}<span className="ml-3 align-middle text-xs font-normal tracking-normal text-muted">{snippets.length}</span></h1></div>
          <div className="flex shrink-0 items-center gap-2">
            <IconButton label="목록 새로고침" onClick={() => void refresh()} disabled={loading || saving}><RefreshCw size={16} className={loading ? "animate-spin" : ""} /></IconButton>
            <button className="primary-button" onClick={create} disabled={saving} aria-label="새 스니펫"><Plus size={15} /><span className="max-[450px]:hidden">새 스니펫</span></button>
          </div>
        </header>

        {!desktop && <div className="flex shrink-0 items-center gap-2 border-b border-[#3e4330] bg-[#26291d] px-8 py-2 text-xs text-[#d0d6ac] max-[600px]:px-4"><Keyboard size={13} /><span>브라우저 미리보기, 전역 치환은 데스크톱 앱에서 사용할 수 있어요</span></div>}

        <div className="flex min-h-0 flex-1 max-[1100px]:relative">
          <section className={`flex min-h-0 min-w-0 flex-1 flex-col ${draft ? "max-[760px]:hidden" : ""}`} aria-label="스니펫 라이브러리">
            <div className="flex shrink-0 items-center justify-between gap-4 px-7 py-5 max-[600px]:flex-wrap max-[600px]:px-4" data-ui-audit-row>
              <label className="flex h-9 min-w-0 max-w-[310px] flex-1 items-center gap-2 rounded-lg border border-line bg-input px-3 focus-within:border-mint"><Search size={14} className="shrink-0 text-muted" /><input ref={searchRef} value={query} onChange={e => setQuery(e.target.value)} placeholder="이름, 단축어, 본문 검색" aria-label="스니펫 검색" className="h-full w-full min-w-0 bg-transparent text-xs outline-none placeholder:text-muted" /><kbd className="shrink-0 text-[12px] text-muted max-[900px]:hidden">Ctrl K</kbd>{query && <button aria-label="검색 지우기" className="flex h-full w-5 shrink-0 items-center justify-center text-muted" onClick={() => setQuery("")}><X size={12} /></button>}</label>
              <button className="secondary-button shrink-0" aria-label="작업 프로필 설정" onClick={() => setProfilesOpen(true)}>{profileLabels[product.effective_profile]} 프로필</button><span className="shrink-0 text-xs text-muted max-[600px]:hidden">{visible.length}개 스니펫</span>
            </div>
            <DataGrid rows={visible} columns={columns} selectedId={draft?.id} loading={loading} error={error} onSelect={open} />
          </section>

          {draft && <aside className={`flex w-[350px] shrink-0 flex-col border-l border-line bg-panel max-[1100px]:w-[310px] max-[760px]:w-full max-[760px]:border-0 ${!draft ? "max-[1100px]:hidden" : ""}`} aria-label="스니펫 편집기">
            <form className="flex min-h-0 flex-1 flex-col" onSubmit={save}>
              <div className="flex h-[76px] shrink-0 items-center justify-between border-b border-line px-6"><div><h2 className="text-xs font-semibold">{snippets.some(s => s.id === draft.id) ? "스니펫 편집" : "새 스니펫"}</h2><p className="mt-1 text-[12px] text-muted">{dirty ? "저장하지 않은 변경사항" : "모든 변경사항이 저장되었어요"}</p></div><IconButton label="편집 취소" disabled={saving} onClick={() => { setDraft(null); setOriginal(""); }}><X size={15} /></IconButton></div>
              <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto px-6 py-6">
                <label className="form-label">이름<input ref={nameRef} className="field" value={draft.name} maxLength={120} onChange={e => setDraft({ ...draft, name: e.target.value })} placeholder="예: 업무용 서명" required disabled={saving} /></label>
                <label className="form-label">단축어<span className="relative"><input aria-label="단축어" aria-describedby="keyword-hint" className="field w-full pr-10 font-mono text-mint" value={draft.keyword} onChange={e => setDraft({ ...draft, keyword: e.target.value })} placeholder=":signature" maxLength={64} required disabled={saving} /><Keyboard size={14} className="absolute right-3 top-3 text-muted" /></span><span id="keyword-hint" className="text-[12px] font-normal leading-4 text-muted">영문, 숫자와 기호를 공백 없이 입력하세요</span></label>
                {warnings.length > 0 && <div className="rounded-lg border border-amber-300/30 bg-amber-300/5 p-3 text-xs leading-5 text-amber-200" aria-live="polite">{warnings.map(w => <p key={w}>{w}</p>)}<p className="mt-2">단축어 또는 실행 방식을 바꾸면 충돌을 줄일 수 있어요</p></div>}
                <label className="form-label">스니펫 프로필<SelectControl aria-label="스니펫 프로필" value={draft.profile ?? "common"} disabled={saving} onChange={e => setDraft({ ...draft, profile: e.target.value })}>{Object.entries(profileLabels).map(([p,label]) => <option key={p} value={p}>{label}</option>)}</SelectControl></label>
                <label className="form-label">실행 방식<SelectControl value={draft.trigger_mode} onChange={e => setDraft({ ...draft, trigger_mode: e.target.value as Trigger })} disabled={saving}>{Object.entries(triggerLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</SelectControl></label>
                <label className="form-label">내용 유형<SelectControl aria-label="내용 유형" value={"PlainText" in draft.content ? draft.template ? "template" : "text" : "vault"} disabled={saving} onChange={e => setDraft({ ...draft, template: e.target.value === "template", content: e.target.value === "vault" ? ("BitwardenRef" in draft.content ? draft.content : { BitwardenRef: { item_id: "", field: "Password" } }) : ("PlainText" in draft.content ? draft.content : { PlainText: "" }) })}><option value="text">일반 텍스트</option><option value="template">빈칸 템플릿</option><option value="vault">Bitwarden 필드</option></SelectControl></label>
                {"PlainText" in draft.content ? <label className="form-label min-h-[200px] flex-1">치환할 내용<textarea aria-label="치환할 내용" className="field min-h-[170px] flex-1 resize-none py-3 text-[13px] leading-7" value={draft.content.PlainText} onChange={e => setDraft({ ...draft, content: { PlainText: e.target.value } })} placeholder="자주 사용하는 문장을 적어 보세요. {{고객명}}, {{날짜}}로 빈칸을 만들 수 있어요" required maxLength={16384} disabled={saving} /><span className="text-right text-[12px] font-normal text-muted">{draft.content.PlainText.length.toLocaleString()}자</span></label> : <VaultReferenceEditor reference={draft.content.BitwardenRef} unlocked={status.vault === "Unlocked"} disabled={saving} onChange={reference => setDraft({ ...draft, content: { BitwardenRef: reference } })} onManage={() => navigate("vault")} />}
                {draft.template && "PlainText" in draft.content && draft.content.PlainText.includes("{{") && <div className="flex flex-col gap-2"><p className="text-[12px] leading-5 text-muted">실행할 때 빈칸을 채워 문장을 완성해요<br />날짜 빈칸에는 오늘 날짜를 미리 채웁니다</p>{templateError && <p role="alert" className="text-xs text-rose-300">{templateError}</p>}<button type="button" className="secondary-button" disabled={!!templateError} onClick={() => setTemplateOpen(true)}>빈칸 입력 시험</button></div>}
                <div className="flex items-center justify-between border-t border-line pt-4"><div><span className="text-xs">스니펫 사용</span><p className="mt-1 text-[12px] text-muted">단축어 자동 치환을 켜거나 꺼요</p></div><button type="button" role="switch" aria-checked={draft.enabled} aria-label="스니펫 사용" className={`toggle ${draft.enabled ? "on" : ""}`} disabled={saving} onClick={() => setDraft({ ...draft, enabled: !draft.enabled })}><span /></button></div>
                {formError && <p className="rounded-lg border border-rose-400/30 bg-rose-400/5 p-3 text-xs leading-5 text-rose-300" role="alert">{formError}</p>}
                {confirmDelete && <div className="rounded-lg border border-rose-400/30 bg-rose-400/5 p-3"><p className="text-xs text-rose-200">이 스니펫을 삭제할까요?</p><div className="mt-3 flex gap-2"><button type="button" className="secondary-button" onClick={() => setConfirmDelete(false)} disabled={saving}>유지</button><button type="button" className="secondary-button text-rose-300" onClick={() => void remove()} disabled={saving}>삭제 확인</button></div></div>}
              </div>
              <div className="flex h-[76px] shrink-0 items-center gap-2 border-t border-line px-6"><button type="button" className="icon-button text-rose-300" aria-label="스니펫 삭제" data-tip="스니펫 삭제" disabled={saving || !snippets.some(s => s.id === draft.id)} onClick={() => setConfirmDelete(v => !v)}><Trash2 size={15} /></button><button type="button" className="secondary-button ml-auto" disabled={saving} onClick={() => { setDraft(null); setOriginal(""); }}>취소</button><button className="primary-button" type="submit" disabled={saving || !dirty}><Check size={14} />{saving ? "저장 중" : "저장"}</button></div>
            </form>
          </aside>}
        </div>
      </main>}
    </div>
    <footer className="flex h-9 shrink-0 items-center gap-4 border-t border-line bg-sidebar px-5 text-[12px] text-muted max-[600px]:gap-2 max-[600px]:px-3">
      <span className="flex min-w-0 items-center gap-2" aria-label={status.error ?? undefined}><span className={`size-1.5 shrink-0 rounded-full ${status.active && !status.paused && !status.error ? "bg-mint" : "bg-[#d8bb79]"}`} /><span className="truncate">{!desktop ? "미리보기 모드" : status.error ? status.error : status.active ? status.paused ? "엔진 일시 정지" : "입력 엔진 실행 중" : "엔진 연결 확인"}</span></span>
      {desktop && <button className="flex items-center gap-1.5 hover:text-ink disabled:opacity-40" disabled={!status.active} onClick={() => void toggleEngine()} aria-label={status.paused ? "엔진 재개" : "엔진 일시 정지"}>{status.paused ? <Play size={10} /> : <Pause size={10} />}<span className="max-[500px]:hidden">{status.paused ? "재개" : "일시 정지"}</span></button>}
      <button className="ml-auto flex shrink-0 items-center gap-1.5 hover:text-mint" onClick={() => navigate(page === "vault" ? "library" : "vault")} aria-label={page === "vault" ? "스니펫으로 돌아가기" : "보관함 열기"}><LockKeyhole size={12} />{page === "vault" ? "스니펫" : "보관함"}</button>
      <button className="flex shrink-0 items-center gap-1.5 hover:text-ink" aria-label="버퍼 비우기" onClick={() => void clearBuffer().then(() => setToast(desktop ? "입력 버퍼를 지웠어요" : "데스크톱 앱에서 입력 버퍼를 관리해요")).catch(e => setToast(String(e)))}><ShieldCheck size={12} /><span className="max-[600px]:hidden">버퍼 비우기</span></button>
      <button className="flex shrink-0 items-center gap-1.5 hover:text-mint min-[601px]:hidden" onClick={() => navigate("settings")} aria-label="앱 설정"><Settings size={12} />설정</button>
    </footer>
    {profilesOpen && <ProfileSettings settings={product.settings} onClose={() => setProfilesOpen(false)} onSaved={() => { void refresh(); setToast("작업 프로필을 저장했어요"); }} />}
    {templateOpen && draft && "PlainText" in draft.content && <Modal isOpen onClose={() => setTemplateOpen(false)} title="템플릿 입력 시험" storageKey="template-preview"><TemplateForm text={draft.content.PlainText} onCancel={() => setTemplateOpen(false)} submitLabel="확인" onSubmit={async () => { setTemplateOpen(false); setToast("완성 문구를 확인했어요"); }} /></Modal>}
    {toast && <div className="toast" role="status" data-ui-audit-overlay><Check size={15} className="shrink-0 text-mint" /><span>{toast}</span><button onClick={() => setToast("")} aria-label="알림 닫기"><X size={13} /></button></div>}
  </div>;
}
export default App;
