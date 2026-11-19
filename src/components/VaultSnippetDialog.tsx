import { useEffect, useMemo, useRef, useState, type FormEvent } from "react";
import { Check } from "lucide-react";
import { Modal } from "./modal/Modal.jsx";
import { SelectControl } from "./SelectControl";
import { fieldLabel, saveSnippet, triggerLabels, type Snippet, type Trigger, type VaultField, type VaultItem } from "../api";
import { diagnoseSnippet, profileLabels, snippetConflicts } from "../productivity";

const KEYWORD = /^[\x21-\x7e]{1,64}$/;
const fieldKey = (field: VaultField) => JSON.stringify(field);
const slug = (text: string, max = 16) => text.normalize("NFKD").toLowerCase().replace(/[^a-z0-9]+/g, "").slice(0, max);
const suffixes: Record<string, string> = { Username: "id", Password: "pw", Totp: "otp", Notes: "memo" };
const suffix = (field: VaultField) => typeof field === "object" ? slug(field.Custom, 12) || "field" : suffixes[field];

/** `:github_pw`처럼 항목 이름과 필드로 단축어를 제안한다. 한글 이름은 아이디 앞부분을 사용한다. */
export function suggestKeyword(item: VaultItem, field: VaultField, taken: Set<string>): string {
  const base = slug(item.name) || slug((item.username ?? "").split("@")[0]) || "bw";
  const stem = `:${base}_${suffix(field)}`;
  let keyword = stem;
  for (let n = 2; taken.has(keyword); n++) keyword = `${stem}${n}`;
  return keyword;
}

export const linkedSnippets = (snippets: Snippet[], itemId: string) =>
  snippets.filter(s => "BitwardenRef" in s.content && s.content.BitwardenRef.item_id === itemId);

interface Row { key: string; field: VaultField; checked: boolean; name: string; keyword: string }

function initialRows(item: VaultItem, snippets: Snippet[]): Row[] {
  const taken = new Set(snippets.map(s => s.keyword));
  const hasLinks = linkedSnippets(snippets, item.id).length > 0;
  const preferred = item.fields.find(f => f === "Password") ?? item.fields[0];
  return item.fields.map(field => {
    const keyword = suggestKeyword(item, field, taken); taken.add(keyword);
    return { key: fieldKey(field), field, keyword, name: `${item.name} ${fieldLabel(field)}`, checked: !hasLinks && preferred !== undefined && fieldKey(field) === fieldKey(preferred) };
  });
}

/** 보관함 항목 하나에서 필드별 단축어를 골라 한 번에 스니펫으로 저장한다. */
export function VaultSnippetDialog({ item, snippets, onClose, onSaved }: { item: VaultItem; snippets: Snippet[]; onClose: () => void; onSaved: (count: number) => void }) {
  const [rows, setRows] = useState(() => initialRows(item, snippets));
  const [profile, setProfile] = useState("common");
  const [trigger, setTrigger] = useState<Trigger>("Space");
  const [warnings, setWarnings] = useState<Record<string, string[]>>({});
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const ids = useRef(new Map(item.fields.map(f => [fieldKey(f), crypto.randomUUID()])));
  const focusRef = useRef<HTMLInputElement>(null);
  const linked = useMemo(() => linkedSnippets(snippets, item.id), [snippets, item.id]);
  const drafts = useMemo<Snippet[]>(() => rows.filter(r => r.checked).map(r => ({
    id: ids.current.get(r.key)!, name: r.name.trim(), keyword: r.keyword, content: { BitwardenRef: { item_id: item.id, field: r.field } },
    trigger_mode: trigger, enabled: true, profile,
  })), [rows, trigger, profile, item.id]);

  useEffect(() => {
    let disposed = false;
    const timer = setTimeout(() => {
      void Promise.all(drafts.map(async draft => {
        const remote = KEYWORD.test(draft.keyword) ? await diagnoseSnippet(draft, snippets).catch(e => [String(e)]) : ["단축어는 공백 없이 영문, 숫자와 기호 1~64자로 입력해 주세요"];
        const local = snippetConflicts(draft, drafts.filter(other => other.id !== draft.id));
        return [draft.id, [...new Set([...remote, ...local])]] as [string, string[]];
      })).then(entries => { if (!disposed) setWarnings(Object.fromEntries(entries)); });
    }, 150);
    return () => { disposed = true; clearTimeout(timer); };
  }, [drafts, snippets]);

  const update = (key: string, patch: Partial<Row>) => setRows(current => current.map(r => r.key === key ? { ...r, ...patch } : r));
  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (!drafts.length) { setError("등록할 필드를 하나 이상 선택해 주세요"); return; }
    if (drafts.some(d => !KEYWORD.test(d.keyword))) { setError("단축어는 공백 없이 영문, 숫자와 기호 1~64자로 입력해 주세요"); return; }
    if (drafts.some(d => !d.name)) { setError("스니펫 이름을 입력해 주세요"); return; }
    if (new Set(drafts.map(d => d.keyword)).size !== drafts.length) { setError("선택한 필드마다 다른 단축어를 입력해 주세요"); return; }
    setSaving(true); setError("");
    let count = 0;
    try {
      for (const draft of drafts) {
        await saveSnippet(draft); count++;
        const row = rows.find(r => ids.current.get(r.key) === draft.id);
        // 저장된 행을 다시 선택하면 새 스니펫이 되도록 ID를 바꾼다.
        if (row) { ids.current.set(row.key, crypto.randomUUID()); update(row.key, { checked: false }); }
      }
      onSaved(count); onClose();
    } catch (e) {
      setError(`${count ? `${count}개는 저장했어요. ` : ""}${e instanceof Error ? e.message : String(e)}`);
      if (count) onSaved(count);
    } finally { setSaving(false); }
  };

  const firstChecked = rows.findIndex(r => r.checked);
  return <Modal isOpen title="스니펫으로 등록" size="md" storageKey="vault-snippet" closeOnOverlayClick={false} initialFocusRef={focusRef} onClose={() => { if (!saving) onClose(); }}
    footer={<><button type="button" className="secondary-button" disabled={saving} onClick={onClose}>취소</button><button type="submit" form="vault-snippet-form" className="primary-button" disabled={saving || !drafts.length}><Check size={14} />{saving ? "저장 중" : drafts.length ? `${drafts.length}개 등록` : "등록"}</button></>}>
    <form id="vault-snippet-form" className="flex flex-col gap-5" onSubmit={save} aria-label="보관함 스니펫 등록">
      <div className="min-w-0"><p className="truncate text-sm font-medium">{item.name}</p><p className="mt-1 truncate text-[12px] text-muted">{item.username || "아이디 없음"} · 연결된 스니펫 {linked.length}개</p></div>
      <div className="grid grid-cols-2 gap-3 max-[500px]:grid-cols-1">
        <label className="form-label">실행 방식<SelectControl aria-label="실행 방식" value={trigger} disabled={saving} onChange={e => setTrigger(e.target.value as Trigger)}>{Object.entries(triggerLabels).map(([value, label]) => <option key={value} value={value}>{label}</option>)}</SelectControl></label>
        <label className="form-label">스니펫 프로필<SelectControl aria-label="스니펫 프로필" value={profile} disabled={saving} onChange={e => setProfile(e.target.value)}>{Object.entries(profileLabels).map(([p, label]) => <option key={p} value={p}>{label}</option>)}</SelectControl></label>
      </div>
      <fieldset className="flex flex-col gap-2" disabled={saving}>
        <legend className="mb-2 text-xs font-medium text-muted">등록할 필드</legend>
        {rows.length === 0 && <p className="text-xs text-muted">이 항목에는 연결할 수 있는 필드가 없어요</p>}
        {rows.map((row, index) => {
          const label = fieldLabel(row.field);
          const existing = linked.filter(s => "BitwardenRef" in s.content && fieldKey(s.content.BitwardenRef.field) === row.key);
          const rowWarnings = row.checked ? warnings[ids.current.get(row.key)!] ?? [] : [];
          return <div key={row.key} className={`rounded-lg border p-3 ${row.checked ? "border-[#668c7a] bg-input" : "border-line"}`} data-ui-audit-row>
            <div className="flex min-w-0 items-center gap-3">
              <label className="flex min-w-0 flex-1 cursor-pointer items-center gap-3 text-xs"><input type="checkbox" className="size-4 shrink-0 accent-[#abeacd]" checked={row.checked} aria-label={`${label} 등록`} onChange={e => update(row.key, { checked: e.target.checked })} /><span className="truncate">{label}</span></label>
              {existing.length > 0 && <span className="flex shrink-0 items-center gap-1.5 text-[12px] text-muted">연결됨{existing.slice(0, 2).map(s => <code key={s.id} className="keyword-chip">{s.keyword}</code>)}{existing.length > 2 && `+${existing.length - 2}`}</span>}
            </div>
            {row.checked && <div className="mt-3 grid grid-cols-2 gap-2 max-[500px]:grid-cols-1">
              <label className="form-label">단축어<input ref={index === firstChecked ? focusRef : undefined} aria-label={`${label} 단축어`} className="field font-mono text-mint" value={row.keyword} maxLength={64} spellCheck={false} autoComplete="off" onFocus={e => e.currentTarget.select()} onChange={e => update(row.key, { keyword: e.target.value.trim() })} /></label>
              <label className="form-label">이름<input aria-label={`${label} 스니펫 이름`} className="field" value={row.name} maxLength={120} onChange={e => update(row.key, { name: e.target.value })} /></label>
            </div>}
            {rowWarnings.length > 0 && <div className="mt-2 text-[12px] leading-5 text-amber-200" aria-live="polite">{rowWarnings.map(w => <p key={w}>{w}</p>)}</div>}
          </div>;
        })}
      </fieldset>
      <p className="text-[12px] leading-5 text-muted">스니펫에는 항목 ID와 필드 참조만 저장해요. 값은 입력할 때 보관함에서 가져옵니다</p>
      {error && <p className="rounded-lg border border-rose-400/30 bg-rose-400/5 p-3 text-xs leading-5 text-rose-300" role="alert">{error}</p>}
    </form>
  </Modal>;
}
