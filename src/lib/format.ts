const UNITS = ["B", "KB", "MB", "GB", "TB"];
const num = new Intl.NumberFormat(undefined, { maximumFractionDigits: 1 });
const date = new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" });

export function formatBytes(n: number): string {
  let v = n;
  let i = 0;
  while (v >= 1024 && i < UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${i === 0 ? v : num.format(v)} ${UNITS[i]}`;
}

export function formatDate(ms: number | null): string {
  return ms === null ? "—" : date.format(ms);
}
