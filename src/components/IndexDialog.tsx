import { useEffect, useState } from "react";
import { errMsg, fs } from "../lib/fs";
import type { IndexSettings, RootState } from "../lib/fs";
import { formatDate } from "../lib/format";
import { describeIndex } from "../lib/searchQuery";
import { useIndex } from "../stores/indexStatus";
import { useUi } from "../stores/ui";
import { Modal } from "./Modal";
import { Pause, Play, RotateCw } from "./icons";

const nf = new Intl.NumberFormat();
const STATE_LABEL: Record<RootState, string> = { pending: "Pendiente", scanning: "Indexando", complete: "Completa", interrupted: "Interrumpida (se retoma sola)" };

const lines = (t: string) => t.split(/\r?\n/).map((x) => x.trim()).filter(Boolean);
const exts = (t: string) => [...new Set(t.split(/[\s,;]+/).map((x) => x.trim().replace(/^\*?\./, "").toLowerCase()).filter(Boolean))];

interface Draft {
  enabled: boolean;
  allDrives: boolean;
  drives: string[];
  paths: string;
  names: string;
  rootDirs: string;
  exts: string;
  minutes: string;
}

const toDraft = (s: IndexSettings): Draft => ({
  enabled: s.enabled,
  allDrives: s.drives === null,
  drives: s.drives ?? [],
  paths: s.excludedPaths.join("\n"),
  names: s.excludedNames.join("\n"),
  rootDirs: s.excludedRootDirs.join("\n"),
  exts: s.excludedExts.join(", "),
  minutes: String(s.reconcileMinutes),
});

/** Estado del índice y su configuración: unidades, exclusiones, pausa/reanudación, actualización y reconstrucción. */
export function IndexDialog() {
  const close = useUi((s) => s.closeDialog);
  const toast = useUi((s) => s.toast);
  const status = useIndex((s) => s.status);
  const statusError = useIndex((s) => s.error);
  const poll = useIndex((s) => s.poll);
  const [draft, setDraft] = useState<Draft | null>(null);
  const [fixed, setFixed] = useState<string[]>([]);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [confirmRebuild, setConfirmRebuild] = useState(false);

  useEffect(() => {
    let live = true;
    void poll();
    if (!fs.available) return;
    Promise.all([fs.indexGetSettings(), fs.indexAvailableDrives()])
      .then(([s, d]) => live && (setDraft(toDraft(s)), setFixed(d)))
      .catch((e) => live && setLoadErr(errMsg(e)));
    return () => void (live = false);
  }, [poll]);

  const fail = (e: unknown) => toast(errMsg(e), "error");
  const patch = (p: Partial<Draft>) => setDraft((d) => (d ? { ...d, ...p } : d));
  const act = (run: () => Promise<unknown>, ok?: string) => void run().then(() => (ok && toast(ok, "success"), poll())).catch(fail);

  const save = () => {
    if (!draft) return;
    const minutes = Number(draft.minutes);
    if (!Number.isInteger(minutes) || minutes < 0 || minutes > 525_600) return toast("«Reconciliar cada» debe ser un número entero de minutos (0 = solo al abrir LFind).", "error");
    if (!draft.allDrives && draft.drives.length === 0) return toast("Elige al menos una unidad o marca «Todas las unidades fijas».", "error");
    const settings: IndexSettings = {
      enabled: draft.enabled,
      drives: draft.allDrives ? null : draft.drives,
      excludedPaths: lines(draft.paths),
      excludedNames: lines(draft.names),
      excludedRootDirs: lines(draft.rootDirs),
      excludedExts: exts(draft.exts),
      reconcileMinutes: minutes,
    };
    setSaving(true);
    fs.indexSetSettings(settings)
      .then(() => (toast("Configuración guardada. Se reindexa solo lo que cambió.", "success"), poll()))
      .catch(fail)
      .finally(() => setSaving(false));
  };

  const phase = status?.phase ?? "idle";
  const pct = status?.progress == null ? null : Math.round(status.progress * 100);
  const unavailable = !fs.available || (!status && !!statusError);

  return (
    <Modal
      title="Índice de búsqueda"
      onClose={close}
      wide
      actions={
        <>
          <button className="btn" onClick={close}>Cerrar</button>
          <button className="btn primary" disabled={!draft || saving} onClick={save}>Guardar configuración</button>
        </>
      }
    >
      {unavailable ? (
        <p className="warn">{fs.available ? statusError : "La búsqueda global solo está disponible en la aplicación de escritorio."}</p>
      ) : (
        <div className="ix">
          <section className="ix-sec">
            <div className="ix-state">
              <strong>{describeIndex(status, (n) => nf.format(n))}</strong>
              {status?.currentRoot && phase !== "idle" && <span className="hint"> · {status.currentRoot}</span>}
            </div>
            {phase !== "idle" && (
              <div className="ix-bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct ?? undefined} data-indeterminate={pct === null || undefined}>
                <i style={pct === null ? undefined : { width: `${pct}%` }} />
              </div>
            )}
            <p className="hint">Última actualización: {formatDate(status?.lastUpdate ?? null)}{status && status.skipped > 0 ? ` · ${nf.format(status.skipped)} carpetas omitidas por falta de acceso` : ""}</p>
            <div className="ix-actions">
              {phase === "paused" ? (
                <button className="btn" onClick={() => act(fs.indexResume)}><Play /> Reanudar</button>
              ) : (
                <button className="btn" disabled={phase !== "scanning"} onClick={() => act(fs.indexPause)}><Pause /> Pausar</button>
              )}
              <button className="btn" disabled={phase !== "idle"} onClick={() => act(async () => ((await fs.indexStart()) || toast("Ya hay un indexado en curso.", "info")), undefined)}><RotateCw /> Actualizar ahora</button>
              {confirmRebuild ? (
                <>
                  <button className="btn danger" onClick={() => (setConfirmRebuild(false), act(fs.indexRebuild, "Reconstruyendo el índice en segundo plano."))}>Sí, borrar y reconstruir</button>
                  <button className="btn" onClick={() => setConfirmRebuild(false)}>No</button>
                </>
              ) : (
                <button className="btn" title="Borra el índice y lo vuelve a crear recorriendo las unidades" onClick={() => setConfirmRebuild(true)}>Reconstruir índice…</button>
              )}
            </div>
            <p className="hint">Mientras se indexa puedes seguir usando LFind y buscando: los resultados mejoran a medida que avanza.</p>
          </section>

          {status && status.roots.length > 0 && (
            <section className="ix-sec">
              <h3>Unidades indexadas</h3>
              <div className="ix-table" role="table">
                {status.roots.map((r) => (
                  <div className="ix-row" role="row" key={r.id} data-offline={!r.online || undefined}>
                    <span className="mono" role="cell">{r.path}</span>
                    <span role="cell">{r.online ? STATE_LABEL[r.state] : "Desconectada o fuera del alcance"}</span>
                    <span role="cell">{nf.format(r.files)} archivos · {nf.format(r.dirs)} carpetas</span>
                    <span role="cell">{r.lastComplete ? formatDate(r.lastComplete) : "—"}</span>
                  </div>
                ))}
              </div>
            </section>
          )}

          {status && status.issues.length > 0 && (
            <section className="ix-sec">
              <h3>Directorios omitidos o con error ({status.issues.length}{status.issues.length >= 50 ? "+" : ""})</h3>
              <ul className="ix-issues">
                {status.issues.map((i, k) => <li key={k}><span className="mono">{i.path}</span><span className="hint"> — {i.message}</span></li>)}
              </ul>
              <p className="hint">Los errores de acceso se registran y el indexado continúa. LFind nunca pide permisos de administrador.</p>
            </section>
          )}

          <section className="ix-sec">
            <h3>Configuración</h3>
            {loadErr ? <p className="warn">{loadErr}</p> : !draft ? <p className="hint">Cargando…</p> : (
              <div className="ix-form">
                <label className="check"><input type="checkbox" checked={draft.enabled} onChange={(e) => patch({ enabled: e.target.checked })} /> Mantener el índice de búsqueda global</label>
                <label className="check"><input type="checkbox" checked={draft.allDrives} onChange={(e) => patch({ allDrives: e.target.checked })} /> Todas las unidades fijas ({fixed.length ? fixed.join(" ") : "ninguna detectada"})</label>
                {!draft.allDrives && (
                  <div className="ix-drives">
                    {fixed.map((d) => (
                      <label className="check" key={d}>
                        <input type="checkbox" checked={draft.drives.includes(d)} onChange={(e) => patch({ drives: e.target.checked ? [...draft.drives, d] : draft.drives.filter((x) => x !== d) })} /> {d}
                      </label>
                    ))}
                  </div>
                )}
                <label className="ix-field"><span>Carpetas excluidas (ruta completa, una por línea)</span><textarea rows={2} spellCheck={false} value={draft.paths} onChange={(e) => patch({ paths: e.target.value })} placeholder={"D:\\Privado"} /></label>
                <label className="ix-field"><span>Nombres de carpeta excluidos en cualquier nivel</span><textarea rows={3} spellCheck={false} value={draft.names} onChange={(e) => patch({ names: e.target.value })} /></label>
                <label className="ix-field"><span>Carpetas excluidas solo en la raíz de cada unidad (Windows, ProgramData…)</span><textarea rows={3} spellCheck={false} value={draft.rootDirs} onChange={(e) => patch({ rootDirs: e.target.value })} /></label>
                <label className="ix-field"><span>Extensiones excluidas (separadas por coma)</span><input spellCheck={false} value={draft.exts} onChange={(e) => patch({ exts: e.target.value })} placeholder="tmp, log, bak" /></label>
                <label className="ix-field"><span>Reconciliar cada (minutos; 0 = solo al abrir LFind)</span><input inputMode="numeric" value={draft.minutes} onChange={(e) => patch({ minutes: e.target.value })} /></label>
                <p className="hint">Al cambiar las exclusiones se vuelve a recorrer lo necesario y lo excluido sale del índice.</p>
              </div>
            )}
          </section>
        </div>
      )}
    </Modal>
  );
}
