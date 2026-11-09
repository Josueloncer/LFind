import { memo, useEffect, useMemo, useRef, useState } from "react";
import { formatBytes } from "../lib/format";
import type { TreeNode } from "../lib/fs";
import { squarify } from "../lib/treemap";

/** Bloque del mapa. `id` nulo: resumen que no es una carpeta (no se puede entrar en él). */
export interface TreeBlock {
  key: string;
  label: string;
  value: number;
  /** Carpeta a la que se entra al pulsar, relativa a la que se muestra. */
  folder: string | null;
  /** Se pulsó «Archivos de esta carpeta». */
  files?: boolean;
  kids?: TreeNode[];
}

/** Un solo tono: la intensidad refuerza el tamaño y el nombre identifica cada bloque, así sirve en los seis temas. */
const shade = (v: number, max: number) => 0.14 + 0.5 * Math.sqrt(max > 0 ? v / max : 0);
const GAP = 1; // 1 px por lado: 2 px entre bloques vecinos

interface Props {
  blocks: TreeBlock[];
  total: number;
  onFolder(rel: string): void;
  onFiles(): void;
}

export const StorageTreemap = memo(function StorageTreemap({ blocks, total, onFolder, onFiles }: Props) {
  const box = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 0, h: 0 });
  const [hover, setHover] = useState<string | null>(null);

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(([e]) => setSize({ w: Math.floor(e.contentRect.width), h: Math.floor(e.contentRect.height) }));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const laid = useMemo(() => squarify(blocks, size.w, size.h), [blocks, size]);
  const max = laid[0]?.value ?? 0;
  const pct = (v: number) => (total > 0 ? `${((v / total) * 100).toFixed(1)} %` : "");
  const pick = (b: TreeBlock) => (b.files ? onFiles() : b.folder !== null && onFolder(b.folder));

  return (
    <figure className="an-map-wrap">
      <div className="an-map" ref={box} role="group" aria-label="Mapa del espacio por carpetas" onPointerLeave={() => setHover(null)}>
        {laid.map((b) => {
          const showKids = !!b.kids?.length && b.w > 150 && b.h > 90;
          const inner = showKids ? squarify(b.kids!.map((k) => ({ ...k, value: k.bytes })), b.w - 8, b.h - 28) : [];
          const clickable = b.files || b.folder !== null;
          return (
            <button
              key={b.key} className="an-tile" data-static={clickable ? undefined : true} disabled={!clickable}
              style={{ left: b.x + GAP, top: b.y + GAP, width: Math.max(0, b.w - 2 * GAP), height: Math.max(0, b.h - 2 * GAP), background: `rgba(var(--accent-rgb), ${shade(b.value, max)})` }}
              aria-label={`${b.label}: ${formatBytes(b.value)}, ${pct(b.value)}`}
              onPointerEnter={() => setHover(`${b.label} · ${formatBytes(b.value)} · ${pct(b.value)}`)}
              onClick={() => pick(b)}
            >
              {b.w > 56 && b.h > 26 && <span className="an-tile-label">{b.label}{showKids && <span className="an-tile-size"> · {formatBytes(b.value)}</span>}</span>}
              {!showKids && b.w > 56 && b.h > 44 && <span className="an-tile-size">{formatBytes(b.value)}</span>}
              {inner.map((k) => (
                <span
                  key={k.id} className="an-sub" role="button" aria-label={`${k.name}: ${formatBytes(k.bytes)}`}
                  style={{ left: k.x + 4 + GAP, top: k.y + 24 + GAP, width: Math.max(0, k.w - 2 * GAP), height: Math.max(0, k.h - 2 * GAP) }}
                  onPointerEnter={(ev) => (ev.stopPropagation(), setHover(`${b.label} › ${k.name} · ${formatBytes(k.bytes)} · ${pct(k.bytes)}`))}
                  onPointerLeave={() => setHover(`${b.label} · ${formatBytes(b.value)} · ${pct(b.value)}`)}
                  onClick={(ev) => { ev.stopPropagation(); if (b.folder !== null) onFolder(`${b.folder}/${k.name}`); }}
                >{k.w > 46 && k.h > 18 ? k.name : ""}</span>
              ))}
            </button>
          );
        })}
      </div>
      <figcaption className="an-map-info">{hover ?? "Pasa el cursor sobre un bloque para ver su tamaño; pulsa para entrar en la carpeta."}</figcaption>
    </figure>
  );
});
