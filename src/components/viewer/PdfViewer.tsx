import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { PDFDocumentProxy, RenderTask } from "pdfjs-dist";
import { ZOOM_STEP, PAGE_GAP, canvasScale, clampZoom, pageAtCenter, pdfDate, pdfScale } from "../../lib/viewerMath";
import type { Box, PdfZoom } from "../../lib/viewerMath";
import { isIllustratorPlaceholder, openPdf, pdfFailure } from "../../lib/pdfEngine";
import type { ViewerFact } from "../../lib/fs";
import { useViewer } from "../../stores/viewer";
import { ChevronDown, ChevronUp, MoveHorizontal, Scan, ZoomIn, ZoomOut } from "../icons";

interface Props {
  url: string;
  size: number;
  illustrator: boolean;
  onFacts(f: ViewerFact[]): void;
  onError(message: string): void;
}

const RENDER_DELAY_MS = 110;

/** Una página. Solo existe mientras está cerca de la zona visible; al salir se cancela su dibujo y se libera el lienzo. */
const Page = memo(function Page({ doc, n, scale, top, w, h, onSize }: { doc: PDFDocumentProxy; n: number; scale: number; top: number; w: number; h: number; onSize(n: number, b: Box): void }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const [drawn, setDrawn] = useState(false);

  useEffect(() => {
    let live = true;
    let task: RenderTask | null = null;
    // Mientras dura el gesto de zoom se reescala el lienzo actual con CSS; se vuelve a dibujar al terminar.
    const timer = setTimeout(async () => {
      try {
        const page = await doc.getPage(n);
        if (!live) return;
        const base = page.getViewport({ scale: 1 });
        onSize(n, { w: base.width, h: base.height });
        const cssW = base.width * scale;
        const cssH = base.height * scale;
        const viewport = page.getViewport({ scale: scale * canvasScale(cssW, cssH, window.devicePixelRatio || 1) });
        const off = document.createElement("canvas");
        off.width = Math.floor(viewport.width);
        off.height = Math.floor(viewport.height);
        const ctx = off.getContext("2d", { alpha: false });
        if (!ctx) return;
        task = page.render({ canvasContext: ctx, viewport });
        await task.promise;
        const c = canvas.current;
        if (!live || !c) return void (off.width = 0);
        c.width = off.width;
        c.height = off.height;
        c.getContext("2d", { alpha: false })?.drawImage(off, 0, 0);
        off.width = 0;
        setDrawn(true);
        page.cleanup();
      } catch {
        /* cancelado, o página dañada: queda el marco en blanco y el resto del documento sigue */
      }
    }, RENDER_DELAY_MS);
    return () => {
      live = false;
      clearTimeout(timer);
      task?.cancel();
    };
  }, [doc, n, scale, onSize]);

  useEffect(() => () => {
    if (canvas.current) canvas.current.width = 0;
  }, []);

  return (
    <div className="vw-page" data-drawn={drawn || undefined} style={{ transform: `translateY(${top}px)`, width: w, height: h }}>
      <canvas ref={canvas} aria-label={`Página ${n}`} />
    </div>
  );
});

/** Visor de PDF con PDF.js. Se dibujan solo las páginas visibles y sus vecinas. */
export default function PdfViewer({ url, size, illustrator, onFacts, onError }: Props) {
  const expanded = useViewer((s) => s.expanded);
  const bind = useViewer((s) => s.bindCommands);
  const scroller = useRef<HTMLDivElement>(null);
  const [doc, setDoc] = useState<PDFDocumentProxy | null>(null);
  // Sin elección del usuario: al ancho en el panel y la página entera en la vista ampliada.
  const [chosen, setZoom] = useState<PdfZoom | null>(null);
  const zoom: PdfZoom = chosen ?? { mode: expanded ? "page" : "width" };
  const [box, setBox] = useState<Box>({ w: 0, h: 0 });
  const [first, setFirst] = useState<Box>({ w: 612, h: 792 });
  const [sizes, setSizes] = useState<Map<number, Box>>(() => new Map());
  const [page, setPage] = useState(1);
  const [draft, setDraft] = useState<string | null>(null);

  useLayoutEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const update = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  useEffect(() => {
    let live = true;
    let release = () => {};
    const open = openPdf(url, size);
    release = open.cancel;
    open.promise
      .then(async ({ doc: d }) => {
        if (!live) return;
        if (illustrator && (await isIllustratorPlaceholder(d))) {
          return onError("Este archivo de Illustrator se guardó sin contenido compatible con PDF, así que no hay un dibujo que mostrar sin Illustrator.");
        }
        const p1 = await d.getPage(1);
        const v = p1.getViewport({ scale: 1 });
        if (!live) return;
        setFirst({ w: v.width, h: v.height });
        setDoc(d);
        const meta = await d.getMetadata().catch(() => null);
        if (!live) return;
        const i = (meta?.info ?? {}) as Record<string, unknown>;
        const text = (k: string) => (typeof i[k] === "string" ? (i[k] as string) : "");
        onFacts([
          { label: "Páginas", value: String(d.numPages) },
          { label: "Tamaño de página", value: `${Math.round((v.width * 25.4) / 72)} × ${Math.round((v.height * 25.4) / 72)} mm` },
          { label: "Título", value: text("Title") },
          { label: "Autor", value: text("Author") },
          { label: "Creado con", value: text("Creator") },
          { label: "Fecha de creación", value: pdfDate(i.CreationDate) ?? "" },
          { label: "Versión de PDF", value: text("PDFFormatVersion") },
        ]);
      })
      .catch((e) => live && onError(pdfFailure(e) === "password" ? "Este PDF está protegido con contraseña." : "No se pudo leer este documento: puede estar dañado."));
    return () => {
      live = false;
      release();
    };
  }, [url, size, illustrator, onFacts, onError]);

  const scale = pdfScale(zoom, first, box);
  const heightOf = useCallback((i: number) => ((sizes.get(i + 1) ?? first).h * scale) + PAGE_GAP, [sizes, first, scale]);
  const count = doc?.numPages ?? 0;
  const vv = useVirtualizer({ count, getScrollElement: () => scroller.current, estimateSize: heightOf, overscan: 1, paddingStart: PAGE_GAP });
  useEffect(() => vv.measure(), [vv, heightOf]);

  const starts = useMemo(() => {
    const out: number[] = [];
    let at = PAGE_GAP;
    for (let i = 0; i < count; i++) (out.push(at), (at += heightOf(i)));
    return out;
  }, [count, heightOf]);

  const onScroll = () => {
    const el = scroller.current;
    if (el && starts.length) setPage(pageAtCenter(starts, el.scrollTop, el.clientHeight));
  };
  const goTo = useCallback((n: number) => doc && vv.scrollToIndex(Math.max(0, Math.min(doc.numPages - 1, n - 1)), { align: "start" }), [doc, vv]);
  const onSize = useCallback((n: number, b: Box) => {
    setSizes((m) => {
      const old = m.get(n);
      if (old && old.w === b.w && old.h === b.h) return m;
      return new Map(m).set(n, b);
    });
  }, []);

  const zoomBy = useCallback((k: number) => setZoom({ mode: "custom", scale: clampZoom(scale * k) }), [scale]);
  useEffect(() => {
    bind({ zoomIn: () => zoomBy(ZOOM_STEP), zoomOut: () => zoomBy(1 / ZOOM_STEP), fit: () => setZoom({ mode: "page" }), actual: () => setZoom({ mode: "custom", scale: 1 }) });
    return () => bind(null);
  }, [bind, zoomBy]);

  useEffect(() => {
    const el = scroller.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return; // sin Ctrl, la rueda recorre el documento
      e.preventDefault();
      zoomBy(e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoomBy]);

  const commit = () => {
    const n = Number(draft);
    if (draft !== null && Number.isInteger(n) && n >= 1 && n <= count) goTo(n);
    setDraft(null);
  };
  const onDraftKey = (e: KeyboardEvent<HTMLInputElement>) => {
    e.stopPropagation();
    if (e.key === "Enter") e.currentTarget.blur();
    if (e.key === "Escape") (setDraft(null), e.currentTarget.blur());
  };

  return (
    <>
      <div ref={scroller} className="vw-doc" tabIndex={0} onScroll={onScroll}>
        <div className="vw-doc-inner" style={{ height: vv.getTotalSize(), minWidth: first.w * scale + 24 }}>
          {doc && vv.getVirtualItems().map((it) => {
            const b = sizes.get(it.index + 1) ?? first;
            return <Page key={it.index} doc={doc} n={it.index + 1} scale={scale} top={it.start} w={b.w * scale} h={b.h * scale} onSize={onSize} />;
          })}
        </div>
      </div>
      {doc && (
        <div className="vw-tools" role="toolbar" aria-label="Controles del documento">
          <button className="ibtn" disabled={page <= 1} title="Página anterior" aria-label="Página anterior" onClick={() => goTo(page - 1)}><ChevronUp /></button>
          <label className="vw-page-field">
            <input aria-label="Página actual" inputMode="numeric" value={draft ?? String(page)} onFocus={(e) => (setDraft(String(page)), e.currentTarget.select())} onChange={(e) => setDraft(e.target.value)} onBlur={commit} onKeyDown={onDraftKey} />
            <span>/ {count}</span>
          </label>
          <button className="ibtn" disabled={page >= count} title="Página siguiente" aria-label="Página siguiente" onClick={() => goTo(page + 1)}><ChevronDown /></button>
          <span className="tb-sep vw-opt" />
          <button className="ibtn vw-opt" title="Alejar (−)" aria-label="Alejar" onClick={() => zoomBy(1 / ZOOM_STEP)}><ZoomOut /></button>
          {expanded && <span className="vw-pct">{Math.round(scale * 100)} %</span>}
          <button className="ibtn vw-opt" title="Acercar (+)" aria-label="Acercar" onClick={() => zoomBy(ZOOM_STEP)}><ZoomIn /></button>
          <span className="tb-sep" />
          <button className="ibtn" aria-pressed={zoom.mode === "width"} title="Ajustar al ancho" aria-label="Ajustar al ancho" onClick={() => setZoom({ mode: "width" })}><MoveHorizontal /></button>
          <button className="ibtn" aria-pressed={zoom.mode === "page"} title="Ajustar a la página (0)" aria-label="Ajustar a la página" onClick={() => setZoom({ mode: "page" })}><Scan /></button>
        </div>
      )}
    </>
  );
}
