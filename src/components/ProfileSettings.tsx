import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { Modal } from "./modal/Modal.jsx";
import { SelectControl } from "./SelectControl";
import { profileLabels, saveProfiles, type ProfileSettings as Settings } from "../productivity";

export function ProfileSettings({ settings, onClose, onSaved }: { settings: Settings; onClose: () => void; onSaved: () => void }) {
  const [draft, setDraft] = useState<Settings>(structuredClone(settings));
  const [error, setError] = useState(""); const [saving, setSaving] = useState(false);
  const save = async () => {
    setSaving(true); setError("");
    try { await saveProfiles(draft); onSaved(); onClose(); }
    catch (error) { setError(String(error)); }
    finally { setSaving(false); }
  };
  return <Modal isOpen onClose={() => { if (!saving) onClose(); }} title="작업 프로필" storageKey="profiles" footer={<button className="primary-button" disabled={saving} onClick={() => void save()}>{saving ? "저장 중" : "프로필 저장"}</button>}>
    <div className="flex flex-col gap-5 text-xs">
      <label className="form-label">기본 프로필<SelectControl aria-label="기본 프로필" value={draft.active_profile} disabled={saving} onChange={e => setDraft({ ...draft, active_profile: e.target.value })}><option value="work">업무</option><option value="personal">개인</option></SelectControl></label>
      <p className="leading-6 text-muted">공통 스니펫은 모든 프로필에서 사용해요<br />등록한 앱에서는 앱 규칙이 기본 프로필보다 먼저 적용됩니다</p>
      <div className="flex items-center justify-between"><h3>앱별 자동 선택</h3><button className="secondary-button" disabled={saving || draft.rules.length >= 100} onClick={() => setDraft({ ...draft, rules: [...draft.rules, { application: "", profile: draft.active_profile }] })}><Plus size={13} />앱 추가</button></div>
      {draft.rules.length === 0 && <p className="py-5 text-center text-muted">앱 규칙을 추가하면 프로필을 자동으로 선택해요</p>}
      {draft.rules.map((rule, i) => <div key={i} className="flex flex-wrap items-center gap-2" data-ui-audit-row>
        <input className="field min-w-0 flex-1" aria-label={`앱 실행 파일 ${i + 1}`} placeholder="예: outlook.exe" value={rule.application} disabled={saving} onChange={e => setDraft({ ...draft, rules: draft.rules.map((r, n) => n === i ? { ...r, application: e.target.value.trim() } : r) })} />
        <SelectControl aria-label={`앱 프로필 ${i + 1}`} value={rule.profile} disabled={saving} onChange={e => setDraft({ ...draft, rules: draft.rules.map((r, n) => n === i ? { ...r, profile: e.target.value } : r) })}>{["work", "personal"].map(p => <option key={p} value={p}>{profileLabels[p]}</option>)}</SelectControl>
        <button className="icon-button h-10 w-10 text-rose-300" disabled={saving} aria-label={`앱 규칙 삭제 ${i + 1}`} onClick={() => setDraft({ ...draft, rules: draft.rules.filter((_, n) => n !== i) })}><Trash2 size={14} /></button>
      </div>)}
      <p className="leading-6 text-muted">앱 규칙은 실행 파일 이름을 기준으로 적용해요<br />브라우저의 웹사이트별 권한과는 별도로 사용합니다</p>
      {error && <p role="alert" className="text-rose-300">{error}</p>}
    </div>
  </Modal>;
}
