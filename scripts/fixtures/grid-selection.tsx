// Synthetic data only. No desktop or vault connection.
import React, { useState } from "react";
import { createRoot } from "react-dom/client";
import { DataGrid } from "../../src/components/DataGrid";
import "../../src/styles.css";

const rows = Array.from({ length: 60 }, (_, index) => ({ id: String(index), name: `테스트 ${String(index).padStart(2, "0")}` }));
const columns = [{ field: "name" as const, headerName: "항목", flex: 1 }];
function Fixture() {
  const [selectedId, setSelectedId] = useState<string>();
  const [search, setSearch] = useState("");
  return <main className="flex h-dvh min-w-0 flex-col bg-canvas text-ink">
    <h1 className="p-4" data-ui-audit-context-title>그리드 선택 검사</h1>
    <div className="flex gap-4 p-4"><input className="min-w-0" aria-label="검색" value={search} onChange={event => setSearch(event.target.value)} /><button onClick={() => setSelectedId(undefined)}>선택 해제</button></div>
    <DataGrid rows={rows.filter(row => row.name.includes(search))} columns={columns} loading={false} error="" selectedId={selectedId} onSelect={row => setSelectedId(row.id)} />
  </main>;
}
createRoot(document.getElementById("root")!).render(<Fixture />);
