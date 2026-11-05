import { useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown, Search } from "lucide-react";
import type { VaultItem } from "../api";
import { searchVaultItems, vaultItemLabel } from "../vaultSearch";

export function VaultItemPicker({ items, value, disabled, loading, onChange }: {
  items: VaultItem[]; value: string; disabled: boolean; loading: boolean; onChange: (item: VaultItem) => void;
}) {
  const id = useId();
  const anchor = useRef<HTMLDivElement>(null);
  const popup = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const [position, setPosition] = useState<{ left: number; top?: number; bottom?: number; width: number; maxHeight: number }>({ left: 0, top: 0, width: 0, maxHeight: 280 });
  const rows = useMemo(() => searchVaultItems(items, query), [items, query]);
  const selected = items.find(item => item.id === value);
  useEffect(() => { setActive(0); }, [query]);
  useEffect(() => { if (disabled || loading) setOpen(false); }, [disabled, loading]);
  useLayoutEffect(() => {
    if (!open) return;
    const locate = () => {
      const rect = anchor.current?.getBoundingClientRect();
      if (!rect) return;
      const below = innerHeight - rect.bottom - 12;
      const height = Math.min(280, Math.max(below, rect.top - 12));
      setPosition({ left: Math.max(8, Math.min(rect.left, innerWidth - rect.width - 8)),
        ...(below >= height ? { top: rect.bottom + 6 } : { bottom: innerHeight - rect.top + 6 }),
        width: Math.min(rect.width, innerWidth - 16), maxHeight: height });
    };
    locate();
    const outside = (event: PointerEvent) => {
      if (!anchor.current?.contains(event.target as Node) && !popup.current?.contains(event.target as Node)) setOpen(false);
    };
    window.addEventListener("resize", locate);
    window.addEventListener("scroll", locate, true);
    document.addEventListener("pointerdown", outside);
    return () => { window.removeEventListener("resize", locate); window.removeEventListener("scroll", locate, true); document.removeEventListener("pointerdown", outside); };
  }, [open]);
  useEffect(() => { if (open) document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: "nearest" }); }, [active, id, open]);
  const choose = (item: VaultItem) => { onChange(item); setOpen(false); setQuery(""); input.current?.focus(); };
  return <div className="min-w-0">
    <label className="form-label" htmlFor={`${id}-input`}>보관함 항목</label>
    <div ref={anchor} className="mt-2 flex h-10 min-w-0 items-center gap-2 rounded-lg border border-input-border bg-input px-3 focus-within:border-mint">
      <Search size={15} className="shrink-0 text-muted" />
      <input ref={input} id={`${id}-input`} role="combobox" aria-autocomplete="list" aria-expanded={open}
        aria-controls={open ? `${id}-list` : undefined} aria-activedescendant={open && rows[active] ? `${id}-${active}` : undefined}
        autoComplete="off" disabled={disabled || loading} className="min-w-0 flex-1 bg-transparent text-sm text-ink outline-none placeholder:text-muted"
        placeholder={loading ? "항목 불러오는 중" : open ? "항목명, 아이디 또는 초성 검색" : "항목 선택 또는 검색"}
        value={open ? query : selected ? vaultItemLabel(selected) : ""}
        onFocus={() => { setOpen(true); setQuery(""); }}
        onChange={event => { setQuery(event.target.value); setOpen(true); }}
        onKeyDown={event => {
          if (event.nativeEvent.isComposing) return;
          if (event.key === "ArrowDown" || event.key === "ArrowUp") {
            event.preventDefault(); setOpen(true);
            setActive(index => !open ? 0 : Math.max(0, Math.min(rows.length - 1, index + (event.key === "ArrowDown" ? 1 : -1))));
          } else if (event.key === "Enter") {
            event.preventDefault(); if (open && rows[active]) choose(rows[active]); else setOpen(true);
          } else if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); setOpen(false); }
          else if (event.key === "Tab") setOpen(false);
        }} />
      <button type="button" className="shrink-0 text-muted hover:text-ink disabled:opacity-50" aria-label="보관함 항목 목록 열기" disabled={disabled || loading}
        onMouseDown={event => event.preventDefault()} onClick={() => { setOpen(value => !value); setQuery(""); input.current?.focus(); }}><ChevronDown size={15} /></button>
    </div>
    {loading && <div className="mt-2 h-2 animate-pulse rounded bg-elevated" role="status" aria-label="보관함 항목 불러오는 중" />}
    {open && !disabled && createPortal(<div ref={popup} data-ui-audit-overlay className="fixed z-[1000] flex min-h-0 flex-col overflow-hidden rounded-xl border border-input-border bg-panel shadow-2xl"
      style={position}>
      <div className="shrink-0 border-b border-line px-3 py-2 text-xs text-muted" role="status">{rows.length}개 항목</div>
      <div id={`${id}-list`} role="listbox" aria-label="보관함 항목 검색 결과" className="min-h-0 overflow-y-auto p-1">
        {rows.length === 0 ? <p className="px-3 py-6 text-center text-sm text-muted">검색 결과가 없어요</p> : rows.map((item, index) =>
          <div key={item.id} id={`${id}-${index}`} role="option" aria-selected={item.id === value}
            className={`flex cursor-pointer items-center gap-2 rounded-lg px-3 py-2.5 text-sm ${index === active ? "bg-selected text-ink" : "text-ink hover:bg-elevated"}`}
            onMouseDown={event => event.preventDefault()} onMouseMove={() => setActive(index)} onClick={() => choose(item)}>
            <span className="min-w-0 flex-1 break-words">{vaultItemLabel(item)}</span>{item.id === value && <Check size={15} className="shrink-0 text-mint" />}
          </div>)}
      </div>
    </div>, document.body)}
  </div>;
}
