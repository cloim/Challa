import { useCallback, useEffect, useMemo, useState, type FormEvent } from "react";
import { ArrowRight, KeyRound, LockKeyhole, LogOut, RefreshCw, Search, ShieldCheck, X } from "lucide-react";
import type { ColDef, ICellRendererParams } from "ag-grid-community";
import { DataGrid } from "./DataGrid";
import { SelectControl } from "./SelectControl";
import { VaultItemPicker } from "./VaultItemPicker";
import { searchVaultItems, vaultItemLabel } from "../vaultSearch";
import { desktop, fieldLabel, lockVault, loginVault, logoutVault, syncVault, unlockVault, vaultInfo, vaultItems, vaultLabels, type VaultField, type VaultInfo, type VaultItem, type VaultReference } from "../api";

export function VaultPanel({ onUse, onStatusChange }: { onUse: (item: VaultItem, field: VaultField) => void; onStatusChange: () => void }) {
  const [info, setInfo] = useState<VaultInfo | null>(null);
  const [items, setItems] = useState<VaultItem[]>([]);
  const [selected, setSelected] = useState<VaultItem | null>(null);
  const [field, setField] = useState<VaultField>("Password");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [server, setServer] = useState("https://vault.bitwarden.com");
  const [customServer, setCustomServer] = useState("");
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [password, setPassword] = useState("");
  const refresh = useCallback(async (inspect = false) => {
    const current = await vaultInfo(inspect); setInfo(current);
    if (current.status === "Unlocked") setItems(await vaultItems());
    else { setItems([]); setSelected(null); }
  }, []);
  useEffect(() => { void refresh(true).catch(e => setError(String(e))).finally(() => setLoading(false)); }, [refresh]);
  useEffect(() => {
    if (!desktop) return;
    const timer = setInterval(() => { void vaultInfo().then(async current => { if (current.status === "Unlocked" && info?.status !== "Unlocked") await refresh(); else setInfo(current); if (current.status !== "Unlocked") { setItems([]); setSelected(null); } }).catch(e => setError(String(e))); }, 2000);
    return () => clearInterval(timer);
  }, [info?.status, refresh]);
  const act = async (action: () => Promise<unknown>) => {
    setBusy(true); setError("");
    try { await action(); await refresh(); }
    catch (e) { setError(String(e)); await refresh().catch(() => {}); }
    finally { setBusy(false); onStatusChange(); }
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (info?.status === "Unauthenticated") {
      const secret = clientSecret; setClientSecret("");
      void act(() => loginVault(server === "custom" ? customServer : server, clientId.trim(), secret));
    } else { const master = password; setPassword(""); void act(() => unlockVault(master)); }
  };
  const rows = useMemo(() => searchVaultItems(items, query), [items, query]);
  const columns = useMemo<ColDef<VaultItem>[]>(() => [
    { field: "name", headerName: "항목", minWidth: 180, flex: 1.3, cellRenderer: ({ data }: ICellRendererParams<VaultItem>) => data && <div className="flex h-full items-center gap-3"><span className="snippet-icon"><KeyRound size={16} /></span><span className="truncate">{vaultItemLabel(data)}</span></div> },
    { field: "username", headerName: "사용자 이름", minWidth: 160, flex: 1, valueFormatter: ({ value }) => value || "사용자 이름 없음" },
    { headerName: "연결 가능 필드", minWidth: 170, flex: 1.1, valueGetter: ({ data }) => data?.fields.map(fieldLabel).join(", ") },
  ], []);
  return <main className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label="Bitwarden 보관함">
    <header className="flex h-[76px] shrink-0 items-center justify-between gap-2 border-b border-line px-8 max-[600px]:h-[76px] max-[600px]:px-4">
      <div><h1 className="text-xl font-semibold tracking-tight" data-ui-audit-context-title>Bitwarden</h1></div>
      <div className="flex items-center gap-1">
        <button className="icon-button" aria-label="보관함 새로고침" data-tip="보관함 새로고침" disabled={busy || loading} onClick={() => void act(() => vaultInfo(true))}><RefreshCw size={16} className={busy ? "animate-spin" : ""} /></button>
        {info?.status === "Unlocked" && <><button className="icon-button" aria-label="보관함 동기화" data-tip="보관함 동기화" disabled={busy} onClick={() => void act(syncVault)}><ShieldCheck size={16} /></button><button className="icon-button" aria-label="보관함 잠그기" data-tip="보관함 잠그기" onClick={() => { setItems([]); setSelected(null); void act(lockVault); }}><LockKeyhole size={16} /></button></>}
        {desktop && info && !["Unavailable", "Unauthenticated"].includes(info.status) && <button className="icon-button" aria-label="보관함 로그아웃" data-tip="보관함 로그아웃" disabled={busy} onClick={() => void act(logoutVault)}><LogOut size={16} /></button>}
      </div>
    </header>
    {error && <div role="alert" className="flex shrink-0 items-start gap-3 border-b border-rose-400/30 bg-rose-400/5 px-6 py-3 text-xs leading-5 text-rose-200"><span className="flex-1">{error}</span><button aria-label="보관함 오류 닫기" onClick={() => setError("")}><X size={14} /></button></div>}
    {loading ? <div className="m-8 h-40 animate-pulse rounded-2xl border border-line bg-panel" aria-label="보관함 상태 확인 중" /> : info?.status === "Unlocked" ? <>
      <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 px-7 py-5 max-[600px]:px-4"><label className="flex h-9 min-w-0 max-w-sm flex-1 items-center gap-2 rounded-lg border border-line bg-panel px-3"><Search size={14} className="shrink-0 text-muted" /><input className="min-w-0 flex-1 bg-transparent text-xs outline-none" aria-label="보관함 검색" placeholder="항목 이름, 사용자 이름 검색" value={query} onChange={e => setQuery(e.target.value)} /></label><span className="text-xs text-muted">{rows.length}개 항목</span></div>
      <DataGrid label="보관함 항목 목록" emptyMessage="검색 조건에 맞는 보관함 항목이 없어요" rows={rows} columns={columns} loading={busy} error="" selectedId={selected?.id} onSelect={item => { setSelected(item); setField(item.fields.includes("Password") ? "Password" : item.fields[0] ?? "Username"); }} />
      {selected && <div className="flex shrink-0 flex-wrap items-end gap-3 border-t border-line bg-panel px-7 py-4 max-[600px]:px-4"><label className="form-label min-w-0 flex-1"><span className="truncate">{selected.name}</span><SelectControl aria-label="연결할 필드" value={JSON.stringify(field)} onChange={e => setField(JSON.parse(e.target.value) as VaultField)}>{selected.fields.map(f => <option key={JSON.stringify(f)} value={JSON.stringify(f)}>{fieldLabel(f)}</option>)}</SelectControl></label><button className="primary-button" disabled={!selected.fields.length || busy} onClick={() => onUse(selected, field)}>스니펫으로 연결<ArrowRight size={14} /></button></div>}
      <p className="shrink-0 border-t border-line px-7 py-3 text-[12px] leading-5 text-muted">준비된 필드 {info.cached_fields}개 | PC 잠금과 해제에 맞춰 보관함을 잠그고 복원해요</p>
    </> : <div className="flex min-h-0 flex-1 overflow-y-auto p-8 max-[600px]:p-4">
      <div className="m-auto w-full max-w-[440px] rounded-2xl border border-line bg-panel p-7 max-[400px]:p-5">
        <div className="mb-5 flex items-center gap-3"><span className="snippet-icon text-mint"><LockKeyhole size={20} /></span><div><h2 className="text-sm font-medium">{!desktop ? "데스크톱에서 보관함을 연결하세요" : info ? vaultLabels[info.status] : "연결 상태를 확인해 주세요"}</h2></div></div>
        {!desktop ? <p className="text-xs leading-6 text-muted">브라우저 미리보기에서는 인증 정보를 받지 않아요<br />Challa 데스크톱 앱에서 Bitwarden을 연결해 주세요</p> : !info?.cli_available ? <p className="text-xs leading-6 text-muted">공식 Bitwarden CLI를 설치하고 Challa를 다시 실행해 주세요<br /><code className="select-text text-mint">npm install -g @bitwarden/cli</code></p> : <form className="flex flex-col gap-4" onSubmit={submit}>
          {info.status === "Unauthenticated" ? <><label className="form-label">서버<SelectControl aria-label="Bitwarden 서버" value={server} onChange={e => setServer(e.target.value)} disabled={busy}><option value="https://vault.bitwarden.com">Bitwarden 미국</option><option value="https://vault.bitwarden.eu">Bitwarden 유럽</option><option value="custom">직접 호스팅한 서버</option></SelectControl></label>{server === "custom" && <label className="form-label">서버 주소<input className="field" type="url" placeholder="https://vault.example.com" value={customServer} onChange={e => setCustomServer(e.target.value)} required disabled={busy} /></label>}<label className="form-label">Client ID<input className="field" autoComplete="off" spellCheck={false} value={clientId} onChange={e => setClientId(e.target.value)} placeholder="user.…" required disabled={busy} /></label><label className="form-label">Client Secret<input className="field" type="password" autoComplete="off" value={clientSecret} onChange={e => setClientSecret(e.target.value)} required disabled={busy} /></label><p className="text-xs leading-5 text-muted">개인 API 키는 Bitwarden 웹 앱의 계정 설정에서 확인할 수 있어요</p></> : <label className="form-label">마스터 비밀번호<input className="field" type="password" autoComplete="off" value={password} onChange={e => setPassword(e.target.value)} required disabled={busy} /></label>}
          <button className="primary-button mt-1 w-full" disabled={busy} type="submit">{busy ? "연결 중" : info.status === "Unauthenticated" ? "로그인" : "잠금 해제"}<ArrowRight size={14} /></button>
        </form>}
      </div>
    </div>}
  </main>;
}

export function VaultReferenceEditor({ reference, unlocked, disabled, onChange, onManage }: { reference: VaultReference; unlocked: boolean; disabled: boolean; onChange: (r: VaultReference) => void; onManage: () => void }) {
  const [items, setItems] = useState<VaultItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let current = true;
    if (!unlocked) { setItems([]); return; }
    setLoading(true); setError("");
    void vaultItems().then(rows => { if (current) setItems(rows); }).catch(e => { if (current) setError(String(e)); }).finally(() => { if (current) setLoading(false); });
    return () => { current = false; };
  }, [unlocked, retry]);
  const item = items.find(i => i.id === reference.item_id);
  return <div className="flex min-w-0 flex-col gap-4 rounded-xl border border-line bg-input p-4">
    <VaultItemPicker items={items} value={reference.item_id} loading={loading} disabled={disabled || !unlocked}
      onChange={selected => onChange({ item_id: selected.id, field: selected.fields.find(f => f === "Password") ?? selected.fields[0] ?? "Username" })} />
    <label className="form-label">필드
      <SelectControl aria-label="연결할 필드" value={JSON.stringify(reference.field)} disabled={disabled || !item}
        onChange={event => onChange({ ...reference, field: JSON.parse(event.target.value) as VaultField })}>
        {!item && <option value={JSON.stringify(reference.field)}>{fieldLabel(reference.field)}</option>}
        {item?.fields.map(field => <option key={JSON.stringify(field)} value={JSON.stringify(field)}>{fieldLabel(field)}</option>)}
      </SelectControl>
    </label>
    {error && <div role="alert" className="flex items-center gap-2 text-xs text-rose-300"><span className="flex-1">{error}</span><button type="button" className="secondary-button" onClick={() => setRetry(value => value + 1)}>다시 시도</button></div>}
    {!unlocked && <p className="text-xs text-muted">보관함 잠금을 해제해 주세요</p>}
    <button type="button" className="secondary-button" onClick={onManage}>보관함 관리<ArrowRight size={13} /></button>
  </div>;
}
