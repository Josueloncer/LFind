import { useEffect, useState } from "react";
import { kindLabel } from "../lib/fileKinds";
import { formatBytes, formatDate } from "../lib/format";
import { errMsg, fs } from "../lib/fs";
import type { Entry, TextPreview } from "../lib/fs";
import { join } from "../lib/path";
import { SHELL, isTextPreviewable, isThumbEligible } from "../lib/thumbKinds";
import { useThumb } from "../hooks/useThumb";
import { EntryGlyph } from "./Glyphs";

const SIZE = 512;

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
    <>
      <pre className="preview-text">{t.text || "(archivo vacío)"}</pre>
      {t.truncated && <p className="preview-note">Se muestran los primeros {formatBytes(t.text.length)} de {formatBytes(t.totalBytes)}.</p>}
    </>
  );
}

function ImageBody({ entry, path }: { entry: Entry; path: string }) {
  const key = `${path}|${SIZE}|${entry.modified ?? 0}|${entry.size}`;
  const { url, failure } = useThumb(key, path, SIZE, true);
  if (url) return <div className="preview-img"><img src={url} alt="" draggable={false} /></div>;
  const needsSystem = SHELL.includes(entry.ext ?? "");
  return (
    <div className="preview-fallback">
      <EntryGlyph entry={entry} size={96} />
      <p className="preview-note">
        {failure === "unsupported" && needsSystem ? "Windows no tiene un proveedor de vistas previas instalado para este formato (por ejemplo Photoshop, Illustrator o Acrobat)."
          : failure === "unsupported" ? "No hay vista previa para este formato."
          : failure === "toolarge" ? "El archivo es demasiado grande para generar una vista previa."
          : failure === "failed" ? "No se pudo generar la vista previa (¿archivo dañado?)."
          : "Generando vista previa…"}
      </p>
    </div>
  );
}

/** Panel de vista previa del elemento seleccionado. Se carga solo para un elemento a la vez. */
export function PreviewPane({ entry, basePath, count }: { entry: Entry | null; basePath: string; count: number }) {
  if (!entry) {
    return (
      <aside className="preview" aria-label="Vista previa">
        <p className="preview-empty">{count > 1 ? `${count} elementos seleccionados` : "Selecciona un archivo para ver su vista previa."}</p>
      </aside>
    );
  }
  const path = join(basePath, entry.name);
  const isImg = isThumbEligible({ ...entry, hidden: false, size: Math.max(entry.size, 1) });
  return (
    <aside className="preview" aria-label="Vista previa">
      <div className="preview-body">
        {entry.kind === "dir" ? <div className="preview-fallback"><EntryGlyph entry={entry} size={96} /></div>
          : isImg ? <ImageBody key={path} entry={entry} path={path} />
          : isTextPreviewable(entry) ? <TextBody path={path} />
          : <div className="preview-fallback"><EntryGlyph entry={entry} size={96} /><p className="preview-note">Sin vista previa para este tipo de archivo.</p></div>}
      </div>
      <div className="preview-meta">
        <strong title={entry.name}>{entry.name}</strong>
        <span>{kindLabel(entry)}</span>
        {entry.kind === "file" && <span>{formatBytes(entry.size)}</span>}
        <span>Modificado: {formatDate(entry.modified)}</span>
      </div>
    </aside>
  );
}
