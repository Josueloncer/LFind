import { useEffect, useRef, useState } from "react";
import { errMsg, fs } from "../lib/fs";
import type { Entry, Inspection } from "../lib/fs";
import { formatBytes, formatDate } from "../lib/format";
import { inspectionKey, inspections } from "../lib/inspection";
import { useNav } from "../stores/nav";
import { useUi } from "../stores/ui";

const QUERY_DELAY_MS = 120;

function Row({ label, value }: { label: string; value: string | null | undefined }) {
  if (!value) return null;
  return <div><dt>{label}</dt><dd title={value}>{value}</dd></div>;
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return <section className="ins-group"><h4>{title}</h4><dl className="vw-facts">{children}</dl></section>;
}

/** El hash solo se calcula al pulsar el botón; cambiar de archivo lo cancela. */
function HashRow({ info }: { info: Inspection }) {
  const [state, setState] = useState<{ phase: "idle" | "running" | "done" | "error"; pct: number; text: string }>({ phase: "idle", pct: 0, text: "" });
  const job = useRef<{ cancel(): void } | null>(null);
  useEffect(() => () => job.current?.cancel(), [info.path]);
  const start = () => {
    setState({ phase: "running", pct: 0, text: "" });
    job.current = fs.startHash(info.path, (e) => {
      if (e.type === "progress") setState({ phase: "running", pct: e.total ? (e.done / e.total) * 100 : 0, text: "" });
      else if (e.error) setState({ phase: "error", pct: 0, text: e.error });
      else setState(e.hash ? { phase: "done", pct: 100, text: e.hash } : { phase: "idle", pct: 0, text: "" });
    });
  };
  if (state.phase === "running") {
    return (
      <div className="ins-hash">
        <div className="bar" data-indeterminate={state.pct === 0 || undefined}><i style={{ width: `${state.pct}%` }} /></div>
        <button className="btn" onClick={() => job.current?.cancel()}>Cancelar</button>
      </div>
    );
  }
  if (state.phase === "done") {
    return (
      <div className="ins-hash">
        <code title={state.text}>{state.text}</code>
        <button className="btn" onClick={() => void navigator.clipboard.writeText(state.text).then(() => useUi.getState().toast("Hash copiado", "success"))}>Copiar</button>
      </div>
    );
  }
  return (
    <div className="ins-hash">
      {state.phase === "error" && <p className="preview-note">{state.text}</p>}
      <button className="btn" disabled={info.cloud} title={info.cloud ? "Solo está en la nube: calcularlo obligaría a descargarlo" : undefined} onClick={start}>Calcular SHA-256</button>
    </div>
  );
}

/** Detalles del elemento seleccionado. Consulta con un pequeño retraso: recorrer una lista con el teclado no lanza una consulta por fila. */
export function Inspector({ entry, path }: { entry: Entry; path: string }) {
  const key = inspectionKey(path, entry.size, entry.modified);
  const [loaded, setLoaded] = useState<{ key: string; info?: Inspection; error?: string } | null>(() => {
    const hit = inspections.peek(key);
    return hit ? { key, info: hit } : null;
  });
  useEffect(() => {
    if (inspections.peek(key)) return setLoaded({ key, info: inspections.peek(key) });
    let live = true;
    const t = setTimeout(() => {
      inspections.get(key, path).then((info) => live && setLoaded({ key, info })).catch((e) => live && setLoaded({ key, error: errMsg(e) }));
    }, QUERY_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [key, path]);

  const current = loaded?.key === key ? loaded : null;
  if (current?.error) return <p className="preview-empty">{current.error}</p>;
  const info = current?.info;
  if (!info) return <div className="ins" role="status" aria-label="Consultando"><span className="vw-loading" /></div>;
  return (
    <div className="ins">
      <Group title="General">
        <Row label="Nombre" value={info.name} />
        <Row label="Extensión" value={info.ext ? info.ext.toUpperCase() : null} />
        <Row label="Tipo" value={info.kind === "dir" ? "Carpeta" : info.kind === "link" ? "Enlace" : "Archivo"} />
        <Row label="Tamaño" value={info.kind === "file" ? `${formatBytes(info.size)} (${info.size.toLocaleString("es")} bytes)` : null} />
        <Row label="Contiene" value={info.children === null || info.children === undefined ? null : `${info.children.toLocaleString("es")}${info.childrenCapped ? " o más" : ""} elementos`} />
        <Row label="Destino" value={info.linkTarget} />
      </Group>
      <button className="ins-location" title="Abrir la carpeta que lo contiene" onClick={() => useNav.getState().navigate(info.parent)}>{info.parent}</button>
      <Group title="Fechas">
        <Row label="Creado" value={formatDate(info.created)} />
        <Row label="Modificado" value={formatDate(info.modified)} />
        <Row label="Último acceso" value={formatDate(info.accessed)} />
      </Group>
      {info.sections.map((s) => <Group key={s.title} title={s.title}>{s.facts.map((f) => <Row key={f.label} label={f.label} value={f.value} />)}</Group>)}
      <Group title="Permisos y atributos">
        <Row label="Propietario" value={info.owner} />
        <Row label="Escritura" value={info.readOnly ? "Solo lectura" : "Permitida"} />
        <Row label="Atributos" value={info.attributes.join(", ") || "Ninguno"} />
        {info.cloud && <Row label="Disponibilidad" value="Solo en la nube: no se ha leído su contenido" />}
      </Group>
      {info.kind === "file" && <section className="ins-group"><h4>Hash</h4><HashRow key={info.path} info={info} /></section>}
    </div>
  );
}
