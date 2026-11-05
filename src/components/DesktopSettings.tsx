import { useEffect, useState } from "react";
import { Minimize2 } from "lucide-react";
import { desktopSettings, hideToTray, saveDesktopSettings, type DesktopStatus } from "../desktop";

export function DesktopSettings() {
  const [settings, setSettings] = useState<DesktopStatus | null>(null);
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  useEffect(() => { void desktopSettings().then(setSettings).catch(e => setError(String(e))); }, []);
  const change = async (key: "start_with_windows" | "close_to_tray", value: boolean) => {
    if (!settings) return;
    setSaving(true); setSaved(false); setError("");
    try { setSettings(await saveDesktopSettings({ start_with_windows: settings.start_with_windows, close_to_tray: settings.close_to_tray, [key]: value })); setSaved(true); }
    catch (e) { setError(String(e)); }
    finally { setSaving(false); }
  };
  return <main className="flex min-h-0 min-w-0 flex-1 flex-col" aria-label="앱 설정">
    <header className="flex h-[76px] shrink-0 items-center justify-between border-b border-line px-8 max-[600px]:px-4">
      <h1 className="text-xl font-semibold" data-ui-audit-context-title>설정</h1>
      <button className="secondary-button" aria-label="트레이로 숨기기" disabled={!settings?.tray_available || saving} onClick={() => void hideToTray().catch(e => setError(String(e)))}><Minimize2 size={15} />트레이로 숨기기</button>
    </header>
    <div className="min-h-0 flex-1 overflow-y-auto px-8 py-6 max-[600px]:px-4">
      {!settings && !error && <div className="max-w-2xl space-y-5" aria-label="설정을 불러오는 중">{[0, 1].map(i => <div key={i} className="h-20 animate-pulse rounded-xl bg-elevated" />)}</div>}
      {settings && <div className="max-w-2xl divide-y divide-line rounded-xl border border-line bg-panel">
        {([
          ["start_with_windows", "Windows 시작 시 자동 실행", !settings.startup_supported],
          ["close_to_tray", "닫기 버튼으로 트레이에 숨기기", false],
        ] as const).map(([key, label, disabled]) => <div key={key} className="flex min-h-24 items-center justify-between gap-5 px-5 py-5">
          <div><span className="text-sm font-medium" id={`${key}-label`}>{label}</span>{key === "start_with_windows" && <p className="mt-1 text-xs text-muted">{settings.startup_supported ? "트레이에서 시작" : "Windows 배포용 앱에서 설정"}</p>}</div>
          <button type="button" role="switch" aria-checked={settings[key]} aria-labelledby={`${key}-label`} className={`toggle shrink-0 ${settings[key] ? "on" : ""}`} disabled={disabled || saving} onClick={() => void change(key, !settings[key])}><span /></button>
        </div>)}
      </div>}
      {(error || settings?.error) && <p role="alert" className="mt-5 max-w-2xl rounded-lg border border-rose-300/50 bg-rose-300/10 p-4 text-sm text-rose-200">{error || settings?.error}</p>}
      <p role="status" aria-live="polite" className="mt-4 text-xs text-muted">{saving ? "저장 중" : saved ? "설정을 저장했어요" : ""}</p>
    </div>
  </main>;
}
