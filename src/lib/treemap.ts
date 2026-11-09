// Mapa de bloques «squarified»: los rectángulos resultantes son lo más cuadrados posible. Puro, sin React. Ver `treemap.test.ts`.

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

function worst(row: number[], side: number): number {
  const sum = row.reduce((a, b) => a + b, 0);
  const max = Math.max(...row);
  const min = Math.min(...row);
  return Math.max((side * side * max) / (sum * sum), (sum * sum) / (side * side * min));
}

/** Coloca `items` en `w`×`h`, con un área proporcional a `value`. Los valores no positivos no se dibujan. */
export function squarify<T extends { value: number }>(items: readonly T[], w: number, h: number): (T & Rect)[] {
  const list = items.filter((i) => i.value > 0).sort((a, b) => b.value - a.value);
  const total = list.reduce((a, b) => a + b.value, 0);
  if (!list.length || w <= 0 || h <= 0 || total <= 0) return [];
  const scale = (w * h) / total;
  const out: (T & Rect)[] = [];
  let [x, y, rw, rh] = [0, 0, w, h];
  let i = 0;
  while (i < list.length) {
    const side = Math.min(rw, rh);
    const row: T[] = [];
    const areas: number[] = [];
    while (i < list.length) {
      const a = list[i].value * scale;
      if (areas.length && worst([...areas, a], side) > worst(areas, side)) break;
      row.push(list[i]);
      areas.push(a);
      i++;
    }
    const sum = areas.reduce((p, q) => p + q, 0);
    if (rw >= rh) {
      // Columna a la izquierda.
      const cw = sum / rh;
      let cy = y;
      row.forEach((it, k) => {
        const ch = areas[k] / cw;
        out.push({ ...it, x, y: cy, w: cw, h: ch });
        cy += ch;
      });
      x += cw;
      rw -= cw;
    } else {
      // Fila arriba.
      const ch = sum / rw;
      let cx = x;
      row.forEach((it, k) => {
        const cw = areas[k] / ch;
        out.push({ ...it, x: cx, y, w: cw, h: ch });
        cx += cw;
      });
      y += ch;
      rh -= ch;
    }
  }
  return out;
}
