import type { Column, GroupBy, SortDir, SortKey, ViewMode } from "./fs";
import { GROUP_LABEL } from "./group";
import { ICON_PRESETS, VIEW_LABEL } from "./view";
import { nextSort } from "./viewState";
import type { EffectiveView } from "./viewState";
import type { MenuItem } from "../stores/ui";

export interface ViewActions {
  setView(patch: Partial<EffectiveView>): void;
  setColumns(c: Column[]): void;
  applyAll(): void;
  /** ¿Esta carpeta tiene su propia vista (modo y tamaño) recordada? */
  pinned: boolean;
  togglePin(): void;
}

const COLS: [Column, string][] = [["modified", "Fecha de modificación"], ["created", "Fecha de creación"], ["type", "Tipo"], ["size", "Tamaño"]];

export function columnsMenu(columns: Column[], a: Pick<ViewActions, "setColumns">): MenuItem[] {
  return [
    { kind: "header", label: "Columnas" },
    { kind: "item", label: "Nombre", checked: true, disabled: true, run() {} },
    ...COLS.map(([c, label]): MenuItem => ({
      kind: "item", label, checked: columns.includes(c),
      run: () => a.setColumns(columns.includes(c) ? columns.filter((x) => x !== c) : [...columns, c]),
    })),
  ];
}

export interface ThumbActions {
  thumbnails: boolean;
  toggleThumbnails(): void;
  systemIcons: boolean;
  toggleSystemIcons(): void;
  clearThumbCache(): void;
}

/** Tamaños de icono y modos de visualización (lo comparten «Vista» de la barra y «Ver» del menú contextual). */
export function viewModeItems(v: EffectiveView, a: Pick<ViewActions, "setView">): MenuItem[] {
  const nearest = ICON_PRESETS.reduce((b, p) => (Math.abs(p.size - v.iconSize) < Math.abs(b.size - v.iconSize) ? p : b));
  const mode = (m: ViewMode): MenuItem => ({ kind: "item", label: VIEW_LABEL[m], checked: v.viewMode === m, run: () => a.setView({ viewMode: m }) });
  return [
    { kind: "header", label: "Iconos" },
    ...ICON_PRESETS.map((p): MenuItem => ({
      kind: "item", label: p.label, checked: v.viewMode === "icons" && nearest.size === p.size,
      run: () => a.setView({ viewMode: "icons", iconSize: p.size }),
    })),
    { kind: "sep" },
    mode("list"), mode("details"), mode("tiles"), mode("content"),
  ];
}

export function viewMenu(v: EffectiveView, columns: Column[], a: ViewActions, t: ThumbActions): MenuItem[] {
  return [
    ...viewModeItems(v, a),
    { kind: "sep" },
    ...columnsMenu(columns, a),
    { kind: "sep" },
    { kind: "item", label: a.pinned ? "Dejar de recordar la vista de esta carpeta" : "Recordar esta vista solo para esta carpeta", run: a.togglePin },
    { kind: "item", label: "Usar esta vista (orden incluido) en todas las carpetas", run: a.applyAll },
    { kind: "sep" },
    { kind: "header", label: "Miniaturas" },
    { kind: "item", label: "Mostrar miniaturas", checked: t.thumbnails, run: t.toggleThumbnails },
    { kind: "item", label: "Usar iconos del sistema", checked: t.systemIcons, run: t.toggleSystemIcons },
    { kind: "item", label: "Borrar la caché de miniaturas e iconos", run: t.clearThumbCache },
  ];
}

const SORTS: [SortKey, string][] = [["name", "Nombre"], ["kind", "Tipo"], ["size", "Tamaño"], ["modified", "Fecha de modificación"], ["created", "Fecha de creación"]];

/** Texto del orden vigente: criterio y qué queda arriba. */
export function sortDescription(v: Pick<EffectiveView, "sortKey" | "sortDir">): string {
  const label = SORTS.find(([k]) => k === v.sortKey)?.[1] ?? "";
  const up = v.sortDir === "asc";
  const end = v.sortKey === "modified" || v.sortKey === "created" ? (up ? "más antiguo primero" : "más reciente primero") : v.sortKey === "size" ? (up ? "más pequeño primero" : "más grande primero") : up ? "de la A a la Z" : "de la Z a la A";
  return `${label}: ${end}`;
}

export function sortMenu(v: EffectiveView, a: Pick<ViewActions, "setView">): MenuItem[] {
  const dir = (d: SortDir, label: string): MenuItem => ({ kind: "item", label, checked: v.sortDir === d, run: () => a.setView({ sortDir: d }) });
  const group = (g: GroupBy): MenuItem => ({ kind: "item", label: GROUP_LABEL[g], checked: v.groupBy === g, run: () => a.setView({ groupBy: g }) });
  return [
    { kind: "header", label: "Ordenar por" },
    // Elegir otra columna usa su sentido natural; volver a elegir la actual lo invierte. La flecha indica el sentido vigente.
    ...SORTS.map(([k, label]): MenuItem => ({ kind: "item", label, checked: v.sortKey === k, shortcut: v.sortKey === k ? (v.sortDir === "asc" ? "↑" : "↓") : undefined, run: () => a.setView(nextSort(v, k)) })),
    { kind: "sep" },
    dir("asc", "Ascendente"),
    dir("desc", "Descendente"),
    { kind: "sep" },
    { kind: "header", label: "Agrupar por" },
    ...(["none", "type", "modified", "created", "size", "name"] as GroupBy[]).map(group),
  ];
}

export function newMenu(a: { newFolder(): void; newFile(): void }): MenuItem[] {
  return [
    { kind: "item", label: "Carpeta", icon: "folder", shortcut: "Ctrl+Mayús+N", run: a.newFolder },
    { kind: "item", label: "Documento de texto", icon: "file", run: a.newFile },
  ];
}
