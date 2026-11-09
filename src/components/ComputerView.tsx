import { useEffect } from "react";
import type { ComponentType, MouseEvent } from "react";
import { driveLabel } from "../lib/addressBar";
import { placeMenu } from "../lib/contextMenu";
import { formatBytes } from "../lib/format";
import type { DriveUsage, Place } from "../lib/fs";
import { driveKindLabel, groupDrives, isPinned, usedPercent } from "../lib/locations";
import { useApp } from "../stores/app";
import { copyText } from "../stores/clipboard";
import { useNav } from "../stores/nav";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import { EmptyState } from "./EmptyState";
import { Disc3, HardDrive, Network, Usb } from "./icons";

const KIND_ICON: Record<string, ComponentType<{ className?: string }>> = { removable: Usb, network: Network, cdrom: Disc3 };
export const driveIcon = (kind: string | null | undefined) => (kind && KIND_ICON[kind]) || HardDrive;

/** Margen para no repetir la consulta de capacidad cada vez que se entra en la vista. */
const USAGE_MAX_AGE_MS = 20_000;

/** Menú común de una unidad. Solo lee datos que ya están en memoria. */
export function openDriveMenu(ev: MouseEvent, d: Place, label: string) {
  ev.preventDefault();
  const ui = useUi.getState();
  const nav = useNav.getState();
  const prefs = usePrefs.getState();
  ui.openMenu(ev.clientX, ev.clientY, placeMenu({
    open: () => nav.navigate(d.path),
    openInNewTab: () => nav.openTab(d.path),
    pinned: isPinned(prefs.favorites, d.path),
    togglePin: () => prefs.toggleFavorite(d.path, label),
    copyPath: () => void copyText(d.path).then((ok) => ui.toast(ok ? "Ruta copiada" : "No se pudo copiar al portapapeles", ok ? "success" : "error")),
    properties: () => ui.openDialog({ type: "properties", paths: [d.path] }),
  }));
}

function DriveCard({ drive, usage, failed }: { drive: Place; usage?: DriveUsage; failed: boolean }) {
  const navigate = useNav((s) => s.navigate);
  const kind = usage?.kind ?? drive.driveKind;
  const Icon = driveIcon(kind);
  const name = driveLabel(drive.path[0] ?? "", usage ?? { label: null, kind: kind ?? "unknown" });
  const pct = usedPercent(usage);
  const detail = usage && pct !== null ? `${formatBytes(usage.free)} libres de ${formatBytes(usage.total)}` : failed ? "Sin medio o no disponible" : "Consultando…";
  return (
    <button className="drive-card" data-off={failed || undefined} title={`${name} — ${detail}`} onClick={() => navigate(drive.path)} onContextMenu={(ev) => openDriveMenu(ev, drive, name)}>
      <Icon className="drive-card-ico" />
      <span className="drive-card-main">
        <span className="drive-card-name">{name}</span>
        <span className="drive-card-kind">{driveKindLabel(kind)}{usage?.fs ? ` · ${usage.fs}` : ""}</span>
        {pct !== null && <span className="drive-bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(pct)} aria-label={`${Math.round(pct)} % usado`} data-low={pct > 90 || undefined}><i style={{ width: `${pct}%` }} /></span>}
        <span className="drive-text">{detail}</span>
      </span>
    </button>
  );
}

/** «Mi equipo»: todas las unidades. Los datos salen del store; no se recorre ningún disco ni se indexa nada. */
export function ComputerView() {
  const drives = useApp((s) => s.drives);
  const usage = useApp((s) => s.usage);
  const errors = useApp((s) => s.usageErrors);

  useEffect(() => {
    const refresh = () => useApp.getState().loadUsage(USAGE_MAX_AGE_MS);
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);

  if (!drives.length) return <EmptyState title="No se encontraron unidades">Conecta una unidad o comprueba los permisos del sistema.</EmptyState>;
  const groups = groupDrives(drives, (d) => usage[d.path]?.kind ?? d.driveKind);
  return (
    <div className="computer">
      {groups.map((g) => (
        <section key={g.title} aria-label={g.title}>
          {groups.length > 1 && <h2 className="computer-title">{g.title}</h2>}
          <div className="drive-grid">
            {g.drives.map((d) => <DriveCard key={d.id} drive={d} usage={usage[d.path]} failed={d.path in errors} />)}
          </div>
        </section>
      ))}
    </div>
  );
}
