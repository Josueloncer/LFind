import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { kindLabel } from "../lib/fileKinds";
import { errMsg, fs } from "../lib/fs";
import type { FileProps, Stats } from "../lib/fs";
import { formatBytes, formatDate } from "../lib/format";
import { basename } from "../lib/path";
import { copyText } from "../stores/clipboard";
import { useOps } from "../stores/ops";
import { useUi } from "../stores/ui";
import { IndexDialog } from "./IndexDialog";
import { Modal } from "./Modal";
import { UpdateDialog } from "./UpdateDialog";

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function DeleteDialog({ paths, permanent }: { paths: string[]; permanent: boolean }) {
  const close = useUi((s) => s.closeDialog);
  const start = useOps((s) => s.start);
  const shown = paths.slice(0, 6).map(basename);
  const confirm = () => {
    close();
    start({ kind: permanent ? "delete" : "trash", sources: paths });
  };
  return (
    <Modal
      title={permanent ? "¿Eliminar de forma permanente?" : paths.length === 1 ? "¿Mover a la Papelera?" : `¿Mover ${paths.length} elementos a la Papelera?`}
      onClose={close}
      actions={
        <>
          <button className="btn" autoFocus={permanent} onClick={close}>Cancelar</button>
          <button className={permanent ? "btn danger" : "btn primary"} autoFocus={!permanent} onClick={confirm}>{permanent ? "Eliminar permanentemente" : "Mover a la Papelera"}</button>
        </>
      }
    >
      <ul className="name-list">
        {shown.map((n, i) => <li key={i}>{n}</li>)}
        {paths.length > shown.length && <li className="more">… y {paths.length - shown.length} más</li>}
      </ul>
      {permanent ? (
        <p className="warn">Esta acción no se puede deshacer. Los elementos no irán a la Papelera.</p>
      ) : (
        <p className="hint">Podrás recuperarlos desde la Papelera de reciclaje de Windows.</p>
      )}
    </Modal>
  );
}

/** Conflicto de nombres durante una copia/movimiento: lo pregunta el backend y espera la respuesta. */
export function ConflictDialog() {
  const job = useOps((s) => s.jobs.find((j) => j.conflict));
  const resolve = useOps((s) => s.resolve);
  const [all, setAll] = useState(false);
  const key = job?.key;
  useEffect(() => setAll(false), [key, job?.conflict?.dst.path]);
  if (!job?.conflict) return null;
  const { src, dst } = job.conflict;
  const answer = (d: "replace" | "skip" | "keepBoth" | "cancel") => resolve(job.key, d, all && d !== "cancel");
  const hint = (a: number | null, b: number | null) => (a === null || b === null || a === b ? "" : a > b ? " · más reciente" : " · más antiguo");
  return (
    <Modal
      title="Ya existe un elemento con este nombre"
      onClose={() => answer("cancel")}
      wide
      actions={
        <>
          <button className="btn" onClick={() => answer("cancel")}>Cancelar operación</button>
          <button className="btn" onClick={() => answer("skip")}>Omitir</button>
          <button className="btn" onClick={() => answer("keepBoth")}>Conservar ambos</button>
          <button className="btn danger" onClick={() => answer("replace")}>Reemplazar</button>
        </>
      }
    >
      <p><strong>{basename(dst.path)}</strong> en <span className="mono">{dst.path.slice(0, dst.path.length - basename(dst.path).length)}</span></p>
      <div className="cmp">
        <div><h3>Existente</h3><p>{formatBytes(dst.size)}</p><p>{formatDate(dst.modified)}{hint(dst.modified, src.modified)}</p></div>
        <div><h3>Nuevo</h3><p>{formatBytes(src.size)}</p><p>{formatDate(src.modified)}{hint(src.modified, dst.modified)}</p></div>
      </div>
      <label className="check"><input type="checkbox" checked={all} onChange={(e) => setAll(e.target.checked)} /> Aplicar a todos los conflictos de esta operación</label>
    </Modal>
  );
}

const Row = ({ k, v }: { k: string; v: ReactNode }) => (v === null || v === undefined || v === "" ? null : <div className="prow"><span>{k}</span><span className="pval">{v}</span></div>);

function PropertiesDialog({ paths }: { paths: string[] }) {
  const close = useUi((s) => s.closeDialog);
  const toast = useUi((s) => s.toast);
  const [props, setProps] = useState<FileProps | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [stats, setStats] = useState<{ s: Stats; done: boolean } | null>(null);
  const single = paths.length === 1;

  useEffect(() => {
    let live = true;
    if (single) fs.getProperties(paths[0]).then((p) => live && setProps(p)).catch((e) => live && setErr(errMsg(e)));
    return () => void (live = false);
  }, [paths, single]);

  const needStats = !single || (props?.isDir ?? false);
  useEffect(() => {
    if (!needStats || (single && !props)) return;
    let live = true;
    const h = fs.startStats(paths, (e) => {
      if (!live) return;
      setStats(e.type === "progress" ? { s: e, done: false } : { s: e.stats, done: !e.cancelled });
    });
    return () => {
      live = false;
      h.cancel(); // al cerrar el diálogo se detiene el cálculo
    };
  }, [needStats, paths, single, props]);

  const parentOf = (p: string) => p.slice(0, Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")) + 1);
  const where = single ? props?.parent ?? "" : [...new Set(paths.map(parentOf))].length === 1 ? parentOf(paths[0]) : "Varias ubicaciones";
  const size = single && props && !props.isDir ? props.size : stats?.s.bytes;
  const attrs = props ? [props.readonly && "Solo lectura", props.hidden && "Oculto", props.system && "Sistema", props.link && "Enlace"].filter(Boolean).join(", ") : "";

  return (
    <Modal
      title={single ? `Propiedades de ${basename(paths[0])}` : `Propiedades · ${paths.length} elementos`}
      onClose={close}
      wide
      actions={
        <>
          <button className="btn" onClick={() => copyText(paths.join("\n")).then((ok) => toast(ok ? "Ruta copiada" : "No se pudo copiar", ok ? "success" : "error"))}>Copiar ruta</button>
          {single && <button className="btn" onClick={() => fs.revealPath(paths[0]).catch((e) => toast(errMsg(e), "error"))}>Mostrar en el Explorador</button>}
          <button className="btn primary" autoFocus onClick={close}>Cerrar</button>
        </>
      }
    >
      {err ? <p className="warn">{err}</p> : (
        <div className="props">
          <Row k="Nombre" v={single ? props?.name : `${paths.length} elementos`} />
          <Row k="Tipo" v={single && props ? kindLabel({ kind: props.isDir ? "dir" : "file", ext: props.ext }) : undefined} />
          <Row k="Ubicación" v={<span className="mono">{where}</span>} />
          <Row k="Tamaño" v={size === undefined ? (needStats ? "Calculando…" : "") : `${formatBytes(size)} (${size.toLocaleString()} bytes)`} />
          {stats && <Row k="Contenido" v={`${plural(stats.s.files, "archivo", "archivos")}, ${plural(stats.s.dirs - (single && props?.isDir ? 1 : 0) < 0 ? 0 : stats.s.dirs - (single && props?.isDir ? 1 : 0), "carpeta", "carpetas")}${stats.done ? "" : " …"}${stats.s.errors ? ` · ${stats.s.errors} sin acceso` : ""}`} />}
          {props && <><Row k="Creado" v={formatDate(props.created)} /><Row k="Modificado" v={formatDate(props.modified)} /><Row k="Último acceso" v={formatDate(props.accessed)} /><Row k="Atributos" v={attrs} /><Row k="Destino del enlace" v={props.linkTarget && <span className="mono">{props.linkTarget}</span>} /></>}
        </div>
      )}
    </Modal>
  );
}

export function Dialogs() {
  const d = useUi((s) => s.dialog);
  const first = useRef(d);
  first.current = d;
  return (
    <>
      {d?.type === "delete" && <DeleteDialog paths={d.paths} permanent={d.permanent} />}
      {d?.type === "properties" && <PropertiesDialog paths={d.paths} />}
      {d?.type === "index" && <IndexDialog />}
      {d?.type === "update" && <UpdateDialog />}
      <ConflictDialog />
    </>
  );
}
