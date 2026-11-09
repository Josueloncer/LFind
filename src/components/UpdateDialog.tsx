import { useEffect } from "react";
import { formatBytes } from "../lib/format";
import { useOps } from "../stores/ops";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import { useUpdate } from "../stores/update";
import { Modal } from "./Modal";

const STATUS: Record<string, string> = {
  checking: "Comprobando actualizaciones...",
  latest: "Tienes la última versión de LFind.",
  available: "Hay una nueva versión disponible.",
  downloading: "Descargando actualización...",
  verifying: "Verificando la firma de la actualización...",
  ready: "Actualización lista. Reiniciar para aplicar.",
  installing: "Instalando actualización...",
};

/** Buscar, descargar y aplicar actualizaciones. Se puede cerrar durante la descarga: el estado vive en `useUpdate`. */
export function UpdateDialog() {
  const close = useUi((s) => s.closeDialog);
  const u = useUpdate();
  const auto = usePrefs((s) => s.autoUpdateCheck);
  const update = usePrefs((s) => s.update);
  const busyOps = useOps((s) => s.jobs.some((j) => j.status !== "done"));
  const { state } = u;

  useEffect(() => {
    useUpdate.getState().loadVersion();
    const p = useUpdate.getState().state.phase;
    if (p === "idle" || p === "latest" || p === "error") void useUpdate.getState().check();
  }, []);

  const info = "info" in state ? state.info : null;
  const failed = state.phase === "error";
  const pct = state.phase === "downloading" && state.total ? Math.min(100, (state.done / state.total) * 100) : null;
  const size = state.phase === "downloading" && state.total ? state.total : info?.size ?? null;

  return (
    <Modal
      title="Buscar actualizaciones"
      onClose={close}
      wide
      actions={
        <>
          {state.phase === "downloading" ? <button className="btn" onClick={u.cancel}>Cancelar descarga</button> : <button className="btn" onClick={close}>Cerrar</button>}
          {(state.phase === "latest" || (failed && !info)) && <button className="btn" onClick={() => void u.check()}>Volver a comprobar</button>}
          {state.phase === "available" && <button className="btn primary" onClick={() => void u.download()}>Actualizar</button>}
          {failed && info && <button className="btn primary" onClick={() => void u.download()}>Reintentar</button>}
          {state.phase === "ready" && <button className="btn primary" disabled={busyOps} title={busyOps ? "Espera a que terminen las operaciones de archivo en curso" : undefined} onClick={() => void u.restart()}>Reiniciar LFind</button>}
        </>
      }
    >
      <div className="up" aria-live="polite">
        <p className={failed ? "warn" : "up-status"}>{failed ? state.message : STATUS[state.phase] ?? ""}</p>
        {(state.phase === "downloading" || state.phase === "verifying") && (
          <>
            <div className="bar" data-indeterminate={pct === null || undefined} role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct === null ? undefined : Math.round(pct)}>
              <i style={pct === null ? undefined : { width: `${pct}%` }} />
            </div>
            {state.phase === "downloading" && <p className="hint">{formatBytes(state.done)}{state.total ? ` de ${formatBytes(state.total)}` : ""}</p>}
          </>
        )}
        <div className="props">
          {u.installed && <div className="prow"><span>Versión instalada</span><span className="pval">{u.installed}</span></div>}
          {info && <div className="prow"><span>Nueva versión</span><span className="pval">{info.version}</span></div>}
          {size !== null && <div className="prow"><span>Tamaño de descarga</span><span className="pval">{formatBytes(size)}</span></div>}
        </div>
        {info?.notes && (
          <section>
            <h3>Novedades</h3>
            <pre className="up-notes">{info.notes}</pre>
          </section>
        )}
        {state.phase === "ready" && busyOps && <p className="hint">Hay operaciones de archivo en curso: podrás reiniciar cuando terminen.</p>}
        <label className="check"><input type="checkbox" checked={auto} onChange={(e) => update({ autoUpdateCheck: e.target.checked })} /> Comprobar automáticamente al iniciar LFind</label>
      </div>
    </Modal>
  );
}
