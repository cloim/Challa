import { useState } from "react";
import { ClientSideRowModelModule, RowStyleModule, ColumnApiModule, ModuleRegistry, themeQuartz, type ColDef, type RowClickedEvent } from "ag-grid-community";
import { AgGridReact } from "ag-grid-react";

ModuleRegistry.registerModules([ClientSideRowModelModule, RowStyleModule, ColumnApiModule]);
const theme = themeQuartz.withParams({
  browserColorScheme: "dark", backgroundColor: "#20262f", foregroundColor: "#f4f7fb",
  headerBackgroundColor: "#28313d", headerTextColor: "#d9e2ee", borderColor: "#475467",
  rowBorder: { color: "#3b4757" }, accentColor: "#abeacd", selectedRowBackgroundColor: "#344d49",
  oddRowBackgroundColor: "#242c36", rowHoverColor: "#333f4d", fontFamily: '"Pretendard Variable", "Malgun Gothic", sans-serif',
  fontSize: 14, headerFontSize: 13, wrapperBorder: false, headerColumnResizeHandleColor: "#65758b",
  spacing: 8, cellHorizontalPadding: 20,
});

/** Owns grid styling, sorting and loading/error/empty states. Search is local to the screen. */
export function DataGrid<T extends { id: string }>({ rows, columns, loading, error, selectedId, onSelect, label = "스니펫 목록", emptyMessage = "검색 조건에 맞는 스니펫이 없어요" }: {
  rows: T[]; columns: ColDef<T>[]; loading: boolean; error: string; selectedId?: string; onSelect: (row: T) => void;
  label?: string; emptyMessage?: string;
}) {
  const [sort, setSort] = useState("");
  return <div className="relative min-h-0 flex-1" data-ui-audit-scroll-root aria-label={`${label}${sort ? `, ${sort}` : ""}`}>
    <AgGridReact<T> theme={theme} rowData={rows} columnDefs={columns} headerHeight={44} rowHeight={78}
      getRowId={({ data }) => data.id} loading={loading} animateRows={false}
      defaultColDef={{ sortable: true, resizable: true, suppressMovable: true }}
      getRowClass={({ data }) => data?.id === selectedId ? "selected-snippet" : undefined}
      onRowClicked={(event: RowClickedEvent<T>) => { if (event.data) onSelect(event.data); }}
      onCellKeyDown={(event) => { if ((event.event as KeyboardEvent)?.key === "Enter" && event.data) onSelect(event.data); }}
      onSortChanged={({ api }) => { const column = api.getColumnState().find(c => c.sort); setSort(column ? `${column.colId} ${column.sort === "asc" ? "오름차순" : "내림차순"}` : ""); }}
      overlayNoRowsTemplate={`<span class="grid-empty">${emptyMessage}</span>`} />
    {loading && <div className="pointer-events-none absolute inset-x-0 bottom-0 top-11 bg-surface" aria-label="스니펫을 불러오는 중">{[0, 1, 2].map(row => <div key={row} className="flex h-[78px] items-center gap-5 border-b border-line px-5"><span className="size-9 animate-pulse rounded-lg bg-elevated" /><span className="h-3 w-1/3 animate-pulse rounded bg-elevated" /><span className="h-5 w-16 animate-pulse rounded bg-elevated" /></div>)}</div>}
    {error && <div className="absolute inset-x-0 bottom-0 top-11 flex flex-col items-center justify-center gap-2 bg-surface p-6 text-center" role="alert"><span className="text-sm text-rose-300">목록을 불러오지 못했어요</span><span className="max-w-sm text-xs text-muted">{error}</span></div>}
  </div>;
}
