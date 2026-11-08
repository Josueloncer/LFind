// Carga de PDF.js bajo demanda y su configuración de seguridad. Este módulo (y la biblioteca) solo se descarga al abrir un PDF.
import * as pdfjs from "pdfjs-dist";
import type { PDFDocumentLoadingTask, PDFDocumentProxy } from "pdfjs-dist";
import workerUrl from "pdfjs-dist/build/pdf.worker.min.mjs?url";

pdfjs.GlobalWorkerOptions.workerSrc = workerUrl;

const CHUNK = 1 << 18;
const MAX_REQUEST = 4 << 20;

/**
 * Lee el PDF a trozos por el protocolo del visor. Sin esto, PDF.js empezaría pidiendo el archivo entero, y un PDF de 1 GB acabaría
 * completo en memoria. Las peticiones largas se parten: el backend limita el tamaño de cada respuesta.
 */
class RangeTransport extends pdfjs.PDFDataRangeTransport {
  private aborter = new AbortController();

  constructor(private url: string, length: number) {
    super(length, null);
  }

  requestDataRange(begin: number, end: number) {
    void this.fetchRange(begin, end).catch(() => {});
  }

  private async fetchRange(begin: number, end: number) {
    for (let at = begin; at < end; at += MAX_REQUEST) {
      const stop = Math.min(end, at + MAX_REQUEST);
      const r = await fetch(this.url, { headers: { Range: `bytes=${at}-${stop - 1}` }, signal: this.aborter.signal });
      if (r.status !== 206 && r.status !== 200) throw new Error(`HTTP ${r.status}`);
      this.onDataRange(at, new Uint8Array(await r.arrayBuffer()));
    }
  }

  abort() {
    this.aborter.abort();
    super.abort();
  }
}

export interface LoadedPdf {
  doc: PDFDocumentProxy;
  destroy(): void;
}

export type PdfFailure = "password" | "corrupt";

export function pdfFailure(e: unknown): PdfFailure {
  return (e as { name?: string } | null)?.name === "PasswordException" ? "password" : "corrupt";
}

/** Una imagen incrustada mayor que esto se omite: decodificarla podría agotar la memoria de la interfaz. */
const MAX_IMAGE_PIXELS = 100_000_000;
const WORKER_GRACE_MS = 150;

/** Abre un PDF con un Worker propio. El contenido no confiable no ejecuta nada: ni JavaScript del documento, ni formularios XFA, ni `eval`. */
export function openPdf(url: string, size: number): { promise: Promise<LoadedPdf>; cancel(): void } {
  const range = new RangeTransport(url, size);
  const worker = new pdfjs.PDFWorker();
  const base = new URL("./pdfjs/", window.location.href).href;
  const task: PDFDocumentLoadingTask = pdfjs.getDocument({
    range,
    worker,
    length: size,
    maxImageSize: MAX_IMAGE_PIXELS,
    rangeChunkSize: CHUNK,
    disableAutoFetch: true,
    disableStream: true,
    isEvalSupported: false,
    enableXfa: false,
    useSystemFonts: false,
    cMapUrl: `${base}cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${base}standard_fonts/`,
  });
  // El cierre ordenado espera a que el Worker termine lo que esté haciendo, y una página enorme puede tardar minutos:
  // tras un margen breve se termina a la fuerza. Con el visor cerrado no debe quedar nada en marcha.
  let cancelled = false;
  const cancel = () => {
    if (cancelled) return;
    cancelled = true;
    range.abort();
    void task.destroy().catch(() => {});
    setTimeout(() => worker.destroy(), WORKER_GRACE_MS);
  };
  return { promise: task.promise.then((doc) => ({ doc, destroy: cancel })), cancel };
}

/** Un AI guardado sin «Crear archivo compatible con PDF» lleva una página con un aviso en lugar del dibujo. */
export async function isIllustratorPlaceholder(doc: PDFDocumentProxy): Promise<boolean> {
  if (doc.numPages !== 1) return false;
  const page = await doc.getPage(1);
  const text = await page.getTextContent();
  page.cleanup();
  const joined = text.items.map((i) => ("str" in i ? i.str : "")).join(" ");
  return /without PDF Content|sin contenido PDF/i.test(joined);
}
