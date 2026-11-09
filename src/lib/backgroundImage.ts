// Prepara para pintar una imagen de fondo ya guardada.

const MAX_SIDE = 1920;

export const fitWithin = (w: number, h: number, max = MAX_SIDE): [number, number] => {
  const k = Math.min(1, max / Math.max(w, h));
  return [Math.max(1, Math.round(w * k)), Math.max(1, Math.round(h * k))];
};

async function toBase64(b: Blob): Promise<string> {
  const bytes = new Uint8Array(await b.arrayBuffer());
  let s = "";
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
}

const MIN_BAKED = 320;

/** Lado mayor de la imagen ya desenfocada: lo desenfocado no tiene detalle que guardar, y al ampliarla no se nota. */
export const bakedSide = (blur: number, viewSide: number): number => {
  const k = blur > 0 ? Math.min(1, 2.5 / blur) : 1;
  return Math.max(MIN_BAKED, Math.min(MAX_SIDE, Math.ceil(viewSide * k)));
};

function blobOf(url: string): Blob {
  const comma = url.indexOf(",");
  const bin = atob(url.slice(comma + 1));
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  return new Blob([bytes], { type: url.slice(5, url.indexOf(";")) });
}

/**
 * Aplica desenfoque y contraste una sola vez, sobre una copia pequeña. Así la imagen que se pinta es ligera y fija,
 * sin un filtro a pantalla completa que ocupe memoria gráfica. Falla si el navegador no sabe filtrar en un lienzo.
 */
export async function bakeBackground(url: string, blur: number, contrast: number, viewSide: number): Promise<string> {
  if (blur <= 0 && contrast === 100) return url;
  const bmp = await createImageBitmap(blobOf(url));
  const side = bakedSide(blur, viewSide);
  const [w, h] = fitWithin(bmp.width, bmp.height, side);
  const canvas = new OffscreenCanvas(w, h);
  const ctx = canvas.getContext("2d");
  if (!ctx || ctx.filter === undefined) {
    bmp.close();
    throw new Error("sin filtros de lienzo");
  }
  const radius = (blur * Math.max(w, h)) / viewSide; // el desenfoque se elige en píxeles de la ventana
  const margin = Math.ceil(radius * 2); // se dibuja algo más grande para que los bordes no se difuminen hacia transparente
  ctx.filter = `blur(${radius}px) contrast(${contrast}%)`;
  ctx.drawImage(bmp, -margin, -margin, w + margin * 2, h + margin * 2);
  bmp.close();
  const blob = await canvas.convertToBlob({ type: "image/webp", quality: 0.82 });
  return `data:${blob.type};base64,${await toBase64(blob)}`;
}
