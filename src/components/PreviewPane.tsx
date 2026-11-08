import { Suspense, lazy, useCallback, useEffect, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent } from "react";
import { kindLabel } from "../lib/fileKinds";
import { formatBytes, formatDate } from "../lib/format";
import { errMsg, fs } from "../lib/fs";
import type { Entry, TextPreview, ViewerFact, ViewerSource } from "../lib/fs";
import { join } from "../lib/path";
import { isThumbEligible, thumbBucket } from "../lib/thumbKinds";
import { mergeFacts } from "../lib/viewerMath";
import { useThumb } from "../hooks/useThumb";
import { useViewerSource } from "../hooks/useViewerSource";
import { usePrefs } from "../stores/prefs";
import { useViewer } from "../stores/viewer";
import { ErrorBoundary } from "./ErrorBoundary";
import { EntryGlyph } from "./Glyphs";
import { ChevronLeft, ChevronRight, ExternalLink, Maximize2, Minimize2, X } from "./icons";
import { ImageViewer } from "./viewer/ImageViewer";
import { MediaViewer } from "./viewer/MediaViewer";

// PDF.js solo se carga la primera vez que se abre un PDF.
const PdfViewer = lazy(() => import("./viewer/PdfViewer"));

const WIDTH_MIN = 220;
const WIDTH_MAX = 900;
const LOADING_THUMB_CSS = 256;

function TextBody({ path }: { path: string }) {
  const [t, setT] = useState<TextPreview | null>(null);
  const [err, setErr] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    setT(null);
    setErr(null);
    fs.getTextPreview(path).then((r) => live && setT(r)).catch((e) => live && setErr(errMsg(e)));
    return () => void (live = false);
  }, [path]);
  if (err) return <p className="preview-note">{err}</p>;
  if (!t) return <p className="preview-note">Cargando…</p>;
  // Se pinta como texto (nunca como HTML): el contenido del archivo no puede ejecutar nada.
  return (
    <div className="vw-text">
      <pre className="preview-text">{t.text || "(archivo vacío)"}</pre>
      {t.truncated && <p className="preview-note">Se muestran los primeros {formatBytes(t.text.length)} de {formatBytes(t.totalBytes)}.</p>}
    </div>
  );
}

function Fallback({ entry, message }: { entry: Entry; message: string }) {
  return (
    <div className="preview-fallback vw-center">
      <EntryGlyph entry={entry} size={96} />
      <p className="preview-note">{message}</p>
    </div>
  );
}

/** Mientras se prepara el archivo: su miniatura, si ya existe o llega antes; si no, el icono sin más. */
function Loading({ entry, path }: { entry: Entry; path: string }) {
  const eligible = isThumbEligible(entry);
  const px = thumbBucket(LOADING_THUMB_CSS, window.devicePixelRatio || 1);
  const { url } = useThumb(eligible ? `${path}|${px}|${entry.modified ?? 0}|${entry.size}` : null, path, px, true);
  return (
    <div className="preview-fallback vw-center" role="status" aria-label="Cargando vista previa">
      {url ? <img className="vw-progressive" src={url} alt="" draggable={false} /> : <EntryGlyph entry={entry} size={96} />}
      <span className="vw-loading" />
    </div>
  );
}

/** Contenido del visor según lo que haya resuelto el backend. */
function Content({ entry, path, source, onFacts }: { entry: Entry; path: string; source: ViewerSource; onFacts(f: ViewerFact[]): void }) {
  const [failed, setFailed] = useState<string | null>(null);
  const fail = useCallback((m: string) => setFailed(m), []);
  if (failed) return <Fallback entry={entry} message={failed} />;
  switch (source.kind) {
    case "image":
      return (
        <ImageViewer
          url={fs.viewerUrl(source.token)}
          onSize={(w, h) => source.width === null && !source.facts.length && onFacts([{ label: "Dimensiones", value: `${w} × ${h} px` }])}
          onError={() => fail("No se pudo mostrar esta imagen: puede estar dañada.")}
        />
      );
    case "pdf":
      return (
        <Suspense fallback={<Loading entry={entry} path={path} />}>
          <PdfViewer url={fs.viewerUrl(source.token)} size={source.size} illustrator={source.illustrator} onFacts={onFacts} onError={fail} />
        </Suspense>
      );
    case "video":
    case "audio":
      return <MediaViewer url={fs.viewerUrl(source.token)} audio={source.kind === "audio"} onFacts={onFacts} onError={() => fail(`WebView2 no puede reproducir ${source.kind === "audio" ? "este audio" : "este vídeo"}: su códec no es compatible.`)} />;
    case "text":
      return <TextBody path={path} />;
    case "unsupported":
      return <Fallback entry={entry} message={source.message} />;
    case "cancelled":
      return null;
  }
}

function sourceFacts(source: ViewerSource | null): ViewerFact[] {
  if (source?.kind !== "image") return source?.kind === "unsupported" ? source.facts : [];
  const own = source.width !== null && !source.facts.length ? [{ label: "Dimensiones", value: `${source.width} × ${source.height} px` }] : [];
  return [...source.facts, ...own];
}

/**
 * Visor integrado: el panel lateral y la vista ampliada son el MISMO elemento con otro aspecto, así al ampliar no se recarga
 * el documento ni se pierde la página, el zoom o la reproducción.
 */
export function PreviewPane({ entry, basePath, count }: { entry: Entry | null; basePath: string; count: number }) {
  const expanded = useViewer((s) => s.expanded);
  const collapse = useViewer((s) => s.collapse);
  const expand = useViewer((s) => s.expand);
  const nav = useViewer((s) => s.nav);
  const savedWidth = usePrefs((s) => s.previewWidth);
  const update = usePrefs((s) => s.update);
  const [liveWidth, setLiveWidth] = useState<number | null>(null);
  const [runtime, setRuntime] = useState<{ key: string | null; facts: ViewerFact[] }>({ key: null, facts: [] });
  const root = useRef<HTMLElement>(null);

  const path = entry ? join(basePath, entry.name) : null;
  const isFile = entry?.kind === "file";
  const key = entry && isFile ? `${path}|${entry.size}|${entry.modified ?? 0}` : null;
  const load = useViewerSource(fs.available ? key : null, path);
  const source = load.status === "ready" ? load.source : null;
  const onFacts = useCallback((facts: ViewerFact[]) => setRuntime({ key, facts }), [key]);

  // Sin nada que mostrar, la vista ampliada no tiene sentido.
  useEffect(() => {
    if (expanded && !entry) collapse();
  }, [expanded, entry, collapse]);

  // Al volver, el teclado debe seguir donde estaba (la lista de la carpeta o los resultados).
  useEffect(() => {
    if (!expanded) return;
    const previous = document.activeElement as HTMLElement | null;
    root.current?.focus({ preventScroll: true });
    return () => previous?.focus?.({ preventScroll: true });
  }, [expanded]);

  // Atajos de la vista ampliada. En captura: mientras está abierta, el teclado no debe actuar sobre la lista que hay debajo.
  useEffect(() => {
    if (!expanded) return;
    const onKey = (e: KeyboardEvent) => {
      const t = e.target as HTMLElement | null;
      if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const v = useViewer.getState();
      const handled: Record<string, (() => void) | undefined> = {
        Escape: v.collapse,
        " ": v.collapse,
        ArrowLeft: () => v.nav?.step(-1),
        ArrowRight: () => v.nav?.step(1),
        Enter: () => path && void fs.openPath(path).catch(() => {}),
        "+": v.commands?.zoomIn,
        "=": v.commands?.zoomIn,
        "-": v.commands?.zoomOut,
        "0": v.commands?.fit,
        "1": v.commands?.actual,
        r: v.commands?.rotate,
        R: v.commands?.rotate,
        Backspace: () => {},
        Delete: () => {},
      };
      const run = handled[e.key];
      if (!run && !(e.key in handled)) return;
      if (e.key === " " && t instanceof HTMLButtonElement) return; // Espacio sobre un botón lo pulsa
      if (e.key === "Enter" && t instanceof HTMLButtonElement) return;
      e.preventDefault();
      e.stopPropagation();
      run?.();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [expanded, path]);

  const startResize = (e: ReactPointerEvent<HTMLDivElement>) => {
    e.preventDefault();
    const startX = e.clientX;
    const startW = root.current?.getBoundingClientRect().width ?? savedWidth;
    let last = startW;
    const move = (ev: PointerEvent) => {
      last = Math.round(Math.min(WIDTH_MAX, Math.max(WIDTH_MIN, startW + startX - ev.clientX)));
      setLiveWidth(last);
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      setLiveWidth(null);
      update({ previewWidth: last });
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  };

  const position = expanded ? nav?.position() ?? null : null;
  const facts = entry
    ? mergeFacts(
        [
          { label: "Tipo", value: kindLabel(entry) },
          { label: "Tamaño", value: isFile ? formatBytes(entry.size) : "" },
        ],
        sourceFacts(source),
        runtime.key === key ? runtime.facts : [],
        [
          { label: "Ubicación", value: basePath },
          { label: "Modificado", value: formatDate(entry.modified) },
        ],
      )
    : [];
  const note = source?.kind === "image" ? source.note : null;
  const close = () => (expanded ? collapse() : update({ previewPane: false }));

  let body;
  if (!entry) body = <p className="preview-empty">{count > 1 ? `${count} elementos seleccionados` : "Selecciona un archivo para ver su vista previa."}</p>;
  else if (!isFile) body = <div className="preview-fallback vw-center"><EntryGlyph entry={entry} size={96} /></div>;
  else if (!fs.available) body = <Fallback entry={entry} message="La vista previa solo está disponible en la aplicación de escritorio." />;
  else if (load.status === "error") body = <Fallback entry={entry} message={load.message} />;
  else if (!source) body = <Loading entry={entry} path={path as string} />;
  else
    body = (
      <ErrorBoundary scope="viewer" resetKey={key}>
        <Content key={key} entry={entry} path={path as string} source={source} onFacts={onFacts} />
      </ErrorBoundary>
    );

  return (
    <aside ref={root} className="preview vw" tabIndex={-1} data-expanded={expanded || undefined} aria-label="Vista previa" style={liveWidth !== null || savedWidth ? ({ "--vw-w": `${liveWidth ?? savedWidth}px` } as React.CSSProperties) : undefined}>
      {!expanded && <div className="vw-resize" onPointerDown={startResize} title="Arrastra para cambiar la anchura" aria-hidden />}
      <header className="vw-head" data-tauri-drag-region={expanded || undefined}>
        <strong className="vw-title" title={entry?.name}>{entry?.name ?? "Vista previa"}</strong>
        {position && <span className="vw-count">{position.index} de {position.total}</span>}
        <span className="vw-actions">
          {expanded && (
            <>
              <button className="ibtn" title="Archivo anterior (←)" aria-label="Archivo anterior" onClick={() => nav?.step(-1)}><ChevronLeft /></button>
              <button className="ibtn" title="Archivo siguiente (→)" aria-label="Archivo siguiente" onClick={() => nav?.step(1)}><ChevronRight /></button>
              <span className="tb-sep" />
            </>
          )}
          {entry && isFile && <button className="ibtn" title="Abrir con la aplicación predeterminada (Intro)" aria-label="Abrir con la aplicación predeterminada" onClick={() => path && void fs.openPath(path).catch(() => {})}><ExternalLink /></button>}
          <button className="ibtn" disabled={!entry} title={expanded ? "Volver al explorador" : "Ampliar (Espacio)"} aria-label={expanded ? "Volver al explorador" : "Ampliar"} onClick={() => (expanded ? collapse() : expand())}>{expanded ? <Minimize2 /> : <Maximize2 />}</button>
          <button className="ibtn" title={expanded ? "Cerrar (Esc)" : "Ocultar el panel"} aria-label={expanded ? "Cerrar" : "Ocultar el panel"} onClick={close}><X /></button>
        </span>
      </header>
      <div className="vw-stage">{body}</div>
      {entry && (
        <footer className="vw-info">
          {note && <p className="vw-note">{note}</p>}
          <dl className="vw-facts">
            {facts.map((f) => (
              <div key={f.label}><dt>{f.label}</dt><dd title={f.value}>{f.value}</dd></div>
            ))}
          </dl>
        </footer>
      )}
    </aside>
  );
}
