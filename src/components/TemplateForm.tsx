import { useMemo, useState } from "react";
import { renderTemplate, templateFields } from "../productivity";

export function TemplateForm({ text, onSubmit, onCancel, submitLabel = "입력" }: { text: string; onSubmit: (values: Record<string, string>) => Promise<void>; onCancel: () => void; submitLabel?: string }) {
  const fields = useMemo(() => templateFields(text), [text]);
  const [values, setValues] = useState<Record<string, string>>(() => Object.fromEntries(fields.map(f => [f, f === "날짜" ? new Date().toLocaleDateString("sv-SE") : ""])));
  const [busy, setBusy] = useState(false); const [error, setError] = useState("");
  let preview = ""; try { preview = renderTemplate(text, values); } catch (e) { preview = String(e); }
  return <form className="flex min-h-0 flex-col gap-4" onSubmit={async e => { e.preventDefault(); setBusy(true); setError(""); try { renderTemplate(text, values); await onSubmit(values); } catch (e) { setError(String(e)); } finally { setBusy(false); } }}>
    {fields.map((f, i) => <label key={f} className="form-label">{f}<input className="field" aria-label={`빈칸 ${f}`} autoFocus={i === 0} value={values[f]} maxLength={16384} required disabled={busy} onChange={e => setValues({ ...values, [f]: e.target.value })} /></label>)}
    <div><p className="mb-2 text-xs text-muted">완성 문구</p><pre className="max-h-44 overflow-auto whitespace-pre-wrap break-words rounded-lg border border-line bg-canvas p-3 font-sans text-xs leading-6">{preview}</pre></div>
    {error && <p className="text-xs text-rose-300" role="alert">{error}</p>}
    <div className="flex justify-end gap-2"><button type="button" className="secondary-button" disabled={busy} onClick={onCancel}>취소</button><button type="submit" className="primary-button" disabled={busy}>{busy ? "처리 중" : submitLabel}</button></div>
  </form>;
}
