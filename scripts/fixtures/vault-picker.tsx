// Synthetic UI fixture. Never connects to a real vault or desktop process.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { VaultItemPicker } from "../../src/components/VaultItemPicker";
import type { VaultItem } from "../../src/api";
import "../../src/styles.css";
const items: VaultItem[] = [
  ...Array.from({ length: 139 }, (_, index) => ({ id: String(index), name: `테스트 계정 ${index}`, username: `user${index}@example.com`, fields: ["Password" as const] })),
  { id: "work", name: "네이버", username: "work@example.com", fields: ["Password"] },
  { id: "personal", name: "네이버", username: "personal@example.com", fields: ["Password"] },
  { id: "empty", name: "사용자 이름 없음", username: null, fields: ["Notes"] },
];
function Fixture() {
  const [selected, setSelected] = useState("");
  const [disabled, setDisabled] = useState(false);
  const [loading, setLoading] = useState(false);
  const [submits, setSubmits] = useState(0);
  return <main className="flex h-dvh min-w-0 flex-col bg-canvas p-4 text-ink">
    <h1 className="text-lg" data-ui-audit-context-title>보관함 항목 선택 검사</h1>
    <div className="mt-4 flex flex-wrap gap-2"><button className="secondary-button" onClick={() => setDisabled(value => !value)}>잠금 전환</button><button className="secondary-button" onClick={() => setLoading(value => !value)}>로딩 전환</button></div>
    <form className="mt-auto w-full max-w-[380px] rounded-xl border border-line bg-panel p-4" onSubmit={event => { event.preventDefault(); setSubmits(value => value + 1); }}>
      <VaultItemPicker items={items} value={selected} disabled={disabled} loading={loading} onChange={item => setSelected(item.id)} />
      <output data-selected className="mt-4 block text-xs">{selected}</output><output data-submits className="block text-xs">{submits}</output>
      <button type="submit" className="primary-button mt-4">저장</button>
    </form>
  </main>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
