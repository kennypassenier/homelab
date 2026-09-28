// The kp-themes module chassis-rs serves at /static/kp/js/datatable.js
// (2.3.0 vendors kp-themes 7.2.0). Its own .d.ts is not served, so the part
// the dashboard uses is copied here from kp-themes js/datatable.d.ts.
export type SortKey = {
  column: number;
  direction: "ascending" | "descending";
};
export type DataTableHandle = {
  element: HTMLElement;
  refresh: () => void;
  sortBy: (keys: readonly SortKey[]) => void;
  view: () => { sorts: SortKey[]; shown: number; total: number };
};
export function dataTable(element: Element): DataTableHandle | null;
export function attachDataTables(
  root?: ParentNode,
  options?: Record<string, unknown>,
): () => void;
