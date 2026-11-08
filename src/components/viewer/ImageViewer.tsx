import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { ZOOM_STEP, clampPan, fitScale, rotateCw, zoomAt } from "../../lib/viewerMath";
import type { Box, ImageView } from "../../lib/viewerMath";
import { useViewer } from "../../stores/viewer";
import { RotateCw, Scan, ZoomIn, ZoomOut } from "../icons";

const INITIAL: ImageView = { fit: true, scale: 1, x: 0, y: 0, rotation: 0 };

/** Imagen con zoom, ajuste, tamaño real, desplazamiento y rotación. Los GIF se animan solos y el SVG se pinta como imagen: sin scripts ni recursos externos. */
export function ImageViewer({ url, onSize, onError }: { url: string; onSize(w: number, h: number): void; onError(): void }) {
  const expanded = useViewer((s) => s.expanded);
  const bind = useViewer((s) => s.bindCommands);
  const area = useRef<HTMLDivElement>(null);
  const [img, setImg] = useState<Box | null>(null);
  const [box, setBox] = useState<Box>({ w: 0, h: 0 });
  const [view, setView] = useState<ImageView>(INITIAL);
  const drag = useRef<{ id: number; x: number; y: number; vx: number; vy: number } | null>(null);

  useLayoutEffect(() => {
    const el = area.current;
    if (!el) return;
    const update = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // Con «ajustar» activo, la escala sigue al tamaño del área y a la rotación.
  useEffect(() => {
    if (img) setView((v) => (v.fit ? { ...v, scale: fitScale(img, box, v.rotation), x: 0, y: 0 } : clampPan(v, img, box)));
  }, [img, box]);

  const zoomBy = useCallback((k: number, px = 0, py = 0) => img && setView((v) => zoomAt(v, v.scale * k, px, py, img, box)), [img, box]);
  const fit = useCallback(() => img && setView((v) => ({ ...v, fit: true, scale: fitScale(img, box, v.rotation), x: 0, y: 0 })), [img, box]);
  const actual = useCallback(() => img && setView((v) => clampPan({ ...v, fit: false, scale: 1, x: 0, y: 0 }, img, box)), [img, box]);
  const rotate = useCallback(() => {
    if (img) setView((v) => {
      const rotation = rotateCw(v.rotation);
      return v.fit ? { ...v, rotation, scale: fitScale(img, box, rotation), x: 0, y: 0 } : clampPan({ ...v, rotation }, img, box);
    });
  }, [img, box]);

  useEffect(() => {
    bind({ zoomIn: () => zoomBy(ZOOM_STEP), zoomOut: () => zoomBy(1 / ZOOM_STEP), fit, actual, rotate });
    return () => bind(null);
  }, [bind, zoomBy, fit, actual, rotate]);

  // Rueda: en la vista ampliada hace zoom; en el panel solo con Ctrl, para no capturar el desplazamiento normal.
  useEffect(() => {
    const el = area.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!expanded && !e.ctrlKey) return;
      e.preventDefault();
      const r = el.getBoundingClientRect();
      zoomBy(e.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP, e.clientX - r.left - r.width / 2, e.clientY - r.top - r.height / 2);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [expanded, zoomBy]);

  const down = (e: PointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    drag.current = { id: e.pointerId, x: e.clientX, y: e.clientY, vx: view.x, vy: view.y };
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    const d = drag.current;
    if (d && img) setView((v) => clampPan({ ...v, x: d.vx + e.clientX - d.x, y: d.vy + e.clientY - d.y }, img, box));
  };
  const up = () => (drag.current = null);

  const canPan = !!img && ((view.rotation % 180 === 0 ? img.w : img.h) * view.scale > box.w + 1 || (view.rotation % 180 === 0 ? img.h : img.w) * view.scale > box.h + 1);
  return (
    <>
      <div ref={area} className="vw-area vw-checker" data-pan={canPan || undefined} onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}>
        <img
          src={url} alt="" draggable={false} decoding="async" data-ready={!!img || undefined}
          onLoad={(e) => {
            const t = e.currentTarget;
            const size = { w: t.naturalWidth || 300, h: t.naturalHeight || 150 };
            setImg(size);
            onSize(size.w, size.h);
          }}
          onError={onError}
          style={img ? { width: img.w, height: img.h, transform: `translate(-50%, -50%) translate(${view.x}px, ${view.y}px) rotate(${view.rotation}deg) scale(${view.scale})`, imageRendering: view.scale >= 3 ? "pixelated" : undefined } : undefined}
        />
      </div>
      <div className="vw-tools" role="toolbar" aria-label="Controles de imagen">
        <button className="ibtn" title="Alejar (−)" aria-label="Alejar" onClick={() => zoomBy(1 / ZOOM_STEP)}><ZoomOut /></button>
        <span className="vw-pct">{Math.round(view.scale * 100)} %</span>
        <button className="ibtn" title="Acercar (+)" aria-label="Acercar" onClick={() => zoomBy(ZOOM_STEP)}><ZoomIn /></button>
        <span className="tb-sep" />
        <button className="ibtn" aria-pressed={view.fit} title="Ajustar a la ventana (0)" aria-label="Ajustar a la ventana" onClick={fit}><Scan /></button>
        <button className="ibtn vw-txt" title="Tamaño real (1)" aria-label="Tamaño real" onClick={actual}>1:1</button>
        <button className="ibtn" title="Girar 90° (R)" aria-label="Girar 90 grados" onClick={rotate}><RotateCw /></button>
      </div>
    </>
  );
}
