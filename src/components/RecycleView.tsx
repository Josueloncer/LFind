import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { confirmText, filterRecycle, recycleBytes, sortRecycle } from "../lib/recycle";
import { errMsg, fs } from "../lib/fs";
import type { RecycleItem } from "../lib/fs";
import { formatBytes, formatDate } from "../lib/format";
import { useOps } from "../stores/ops";
import { useUi } from "../stores/ui";

const ROW = 40;

/** La Papelera real de Windows. Solo se consulta al abrirla y después de cada operación: nada la vigila en segundo plano. */
export function RecycleView() {
  const toast = useUi((s) => s.toast);
  const opsVersion = useOps((s) => s.version);
  const [items, setItems] = useState<RecycleItem[] | null>(null);
  const [total, setTotal] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [asking, setAsking] = useState<"purge" | "empty" | null>(null);
  const [busy, setBusy] = useState(false);
  const list = useRef<HTMLDivElement>(null);

  const load = useCallback(() => {
    fs.recycleList().then((r) => {
      setItems(sortRecycle(r.items));
      setTotal(r.totalBytes);
      setSelected((cur) => new Set(r.items.filter((it) => cur.has(it.id)).map((it) => it.id)));
    }).catch((e) => toast(errMsg(e), "error"));
  }, [toast]);
  useEffect(load, [load, opsVersion]);

  const shown = useMemo(() => filterRecycle(items ?? [], query), [items, query]);
  const virtual = useVirtualizer({ count: shown.length, getScrollElement: () => list.current, estimateSize: () => ROW, overscan: 8 });
  const chosen = shown.filter((it) => selected.has(it.id));
  const allOn = shown.length > 0 && chosen.length === shown.length;

  const toggle = (id: string) => setSelected((cur) => {
    const next = new Set(cur);
    if (!next.delete(id)) next.add(id);
    return next;
  });
  const act = (run: () => Promise<void>, done: string) => {
    setBusy(true);
    run().then(() => {
      toast(done, "success");
      setSelected(new Set());
      useOps.setState((s) => ({ version: s.version + 1 })); // las carpetas abiertas y esta lista se actualizan
    }).catch((e) => toast(errMsg(e), "error")).finally(() => (setBusy(false), setAsking(null)));
  };
  const restore = () => act(() => fs.recycleRestore(chosen.map((it) => it.id)), `Restaurado${chosen.length === 1 ? "" : "s"} ${chosen.length} elemento${chosen.length === 1 ? "" : "s"}`);
  const confirmNow = () => {
    const ids = (asking === "empty" ? items ?? [] : chosen).map((it) => it.id);
    act(() => fs.recyclePurge(ids), asking === "empty" ? "Papelera vaciada" : "Eliminado definitivamente");
  };
  const text = asking ? confirmText(asking, asking === "empty" ? items?.length ?? 0 : chosen.length) : null;

  return (
    <div className="rb">
      <header className="rb-head">
        <div>
          <h2>Papelera de reciclaje</h2>
          <p className="dup-hint">{items === null ? "Consultando…" : `${items.length.toLocaleString("es")} elementos${total !== null ? ` · ${formatBytes(total)}` : ""}`}</p>
        </div>
        <div className="ap-row">
          <button className="btn" disabled={!chosen.length || busy} onClick={restore}>Restaurar</button>
          <button className="btn" disabled={!chosen.length || busy} onClick={() => setAsking("purge")}>Eliminar definitivamente</button>
          <button className="btn" disabled={!items?.length || busy} onClick={() => setAsking("empty")}>Vaciar papelera</button>
        </div>
      </header>
      <input className="rb-search" value={query} spellCheck={false} placeholder="Buscar por nombre o ubicación" aria-label="Buscar en la Papelera" onChange={(e) => setQuery(e.target.value)} />
      <div className="rb-tr rb-th" role="presentation">
        <input type="checkbox" checked={allOn} disabled={!shown.length} aria-label="Seleccionar todos" onChange={() => setSelected(allOn ? new Set() : new Set(shown.map((it) => it.id)))} />
        <span>Nombre</span><span>Ubicación original</span><span className="an-num">Tamaño</span><span>Eliminado</span>
      </div>
      <div className="rb-list" ref={list}>
        {items !== null && !shown.length && <p className="dup-hint dup-none">{query ? "Nada coincide con la búsqueda." : "La Papelera está vacía."}</p>}
        <div style={{ height: virtual.getTotalSize(), position: "relative" }}>
          {virtual.getVirtualItems().map((row) => {
            const it = shown[row.index];
            const parent = it.original.slice(0, Math.max(0, it.original.length - it.name.length - 1));
            return (
              <label key={it.id} className="rb-tr rb-row" data-on={selected.has(it.id) || undefined} style={{ height: ROW, transform: `translateY(${row.start}px)` }}>
                <input type="checkbox" checked={selected.has(it.id)} aria-label={`Seleccionar ${it.name}`} onChange={() => toggle(it.id)} />
                <span className="rb-name" title={it.name}>{it.name}</span>
                <span className="rb-path" title={parent}>{parent}</span>
                <span className="an-num">{it.size !== null ? formatBytes(it.size) : it.entries !== null ? `${it.entries} elem.` : "—"}</span>
                <span className="dup-date">{formatDate(it.deleted > 1e11 ? it.deleted : it.deleted * 1000)}</span>
              </label>
            );
          })}
        </div>
      </div>
      <footer className="rb-foot dup-hint">{chosen.length ? `${chosen.length} seleccionados · ${formatBytes(recycleBytes(chosen))}` : " "}</footer>
      {text && (
        <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && !busy && setAsking(null)}>
          <div className="modal" role="alertdialog" aria-modal="true" aria-label={text.title}>
            <h2>{text.title}</h2>
            <div className="modal-body"><p>{text.detail}</p></div>
            <div className="modal-actions">
              <button className="btn" disabled={busy} onClick={() => setAsking(null)}>Cancelar</button>
              <button className="btn danger" disabled={busy} onClick={confirmNow}>Eliminar</button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
