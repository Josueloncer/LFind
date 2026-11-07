import { useState } from "react";
import { formatBytes } from "../lib/format";
import { basename } from "../lib/path";
import { STATE_LABEL, jobState, useOps } from "../stores/ops";
import type { Job } from "../stores/ops";
import { useUi } from "../stores/ui";
import { X } from "./icons";

function percent(j: Job): number | null {
  const p = j.progress;
  if (j.status === "done") return 100;
  if (j.status === "queued") return 0;
  if (!p) return null;
  if (p.bytesTotal > 0) return Math.min(100, (p.bytesDone / p.bytesTotal) * 100);
  if (p.filesTotal > 0) return Math.min(100, (p.filesDone / p.filesTotal) * 100);
  return null;
}

function JobCard({ j }: { j: Job }) {
  const cancel = useOps((s) => s.cancel);
  const dismiss = useOps((s) => s.dismiss);
  const [open, setOpen] = useState(false);
  const pct = percent(j);
  const p = j.progress;
  const errors = j.result?.errors ?? [];
  const state = jobState(j);
  const sub =
    state === "queued" ? "En cola: empezará cuando termine otra operación"
    : state === "running" ? (j.conflict ? "Esperando tu decisión…" : p?.current || "Preparando…")
    : state === "partial" || state === "failed" ? `${STATE_LABEL[state]} · ${errors.length} error${errors.length === 1 ? "" : "es"}`
    : state === "cancelled" ? `Cancelado · ${j.result?.completed ?? 0} elemento(s) ya procesado(s)`
    : STATE_LABEL[state];
  return (
    <div className="job" data-state={state === "partial" || state === "failed" ? "error" : state === "running" || state === "queued" ? "run" : "ok"}>
      <div className="job-top">
        <strong>{j.title}</strong>
        {j.status === "done" ? (
          <button className="ibtn" title="Cerrar" aria-label="Cerrar" onClick={() => dismiss(j.key)}><X /></button>
        ) : (
          <button className="ibtn" aria-label="Cancelar"
            title={j.status === "queued" ? "Quitar de la cola" : "Cancelar: se detiene tras el elemento en curso (los archivos grandes se interrumpen a mitad); lo ya procesado no se revierte"}
            onClick={() => cancel(j.key)}><X /></button>
        )}
      </div>
      <div className="job-sub" title={sub}>{sub}</div>
      <div className="bar" data-indeterminate={(state === "running" && pct === null) || undefined}><i style={{ width: pct === null ? undefined : `${pct}%` }} /></div>
      {p && j.status === "running" && (
        <div className="job-meta">
          {p.bytesTotal > 0 ? `${formatBytes(p.bytesDone)} de ${formatBytes(p.bytesTotal)}` : (p.filesTotal > 0 ? `${p.filesDone} de ${p.filesTotal}` : `${p.filesDone} eliminados`)}
          {p.bytesTotal > 0 && p.filesTotal > 0 ? ` · ${p.filesDone} de ${p.filesTotal} archivos` : ""}
        </div>
      )}
      {errors.length > 0 && (
        <>
          <button className="link" onClick={() => setOpen((o) => !o)}>{open ? "Ocultar detalles" : "Ver detalles"}</button>
          {open && (
            <ul className="job-errors">
              {errors.slice(0, 30).map((e, i) => <li key={i}><b>{basename(e.path) || "—"}</b> {e.message}</li>)}
              {errors.length > 30 && <li>… y {errors.length - 30} más</li>}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

/** Cola de transferencias + avisos (toasts), abajo a la derecha. */
export function Overlays() {
  const jobs = useOps((s) => s.jobs);
  const toasts = useUi((s) => s.toasts);
  const dismissToast = useUi((s) => s.dismissToast);
  if (!jobs.length && !toasts.length) return null;
  return (
    <div className="overlays">
      {toasts.map((t) => (
        <div key={t.id} className="toast" data-tone={t.tone} role="status">
          <span>{t.text}</span>
          {t.action && <button className="link" onClick={() => (dismissToast(t.id), t.action!.run())}>{t.action.label}</button>}
          <button className="ibtn" aria-label="Cerrar aviso" onClick={() => dismissToast(t.id)}><X /></button>
        </div>
      ))}
      {jobs.map((j) => <JobCard key={j.key} j={j} />)}
    </div>
  );
}
