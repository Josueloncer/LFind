import { useEffect, useState } from "react";
import { errMsg, fs } from "../lib/fs";
import type { WinFeature, WinStatus } from "../lib/fs";
import { useUi } from "../stores/ui";

interface Pending {
  title: string;
  detail: string;
  run(): Promise<WinStatus>;
}

const ROWS: { feature: Exclude<WinFeature, "preferred">; key: keyof WinStatus; label: string; change: string }[] = [
  { feature: "contextFolders", key: "contextFolders", label: "Menú contextual de carpetas", change: "Añade «Abrir en LFind» al clic derecho sobre carpetas y sobre el fondo de una carpeta." },
  { feature: "contextDrives", key: "contextDrives", label: "Menú contextual de unidades", change: "Añade «Abrir en LFind» al clic derecho sobre las unidades." },
  { feature: "startup", key: "startup", label: "Iniciar LFind con Windows", change: "Abre LFind al iniciar sesión." },
];

/** Configuración → Integración con Windows. Nada cambia en el Registro sin pasar antes por la confirmación. */
export function SettingsDialog() {
  const close = useUi((s) => s.closeDialog);
  const toast = useUi((s) => s.toast);
  const [status, setStatus] = useState<WinStatus | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    fs.winStatus().then(setStatus).catch((e) => toast(errMsg(e), "error"));
  }, [toast]);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && (pending ? setPending(null) : close());
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pending, close]);

  const confirm = () => {
    if (!pending) return;
    setBusy(true);
    pending.run().then((s) => {
      setStatus(s);
      setPending(null);
      toast("Configuración de Windows actualizada.", "success");
    }).catch((e) => toast(errMsg(e), "error")).finally(() => setBusy(false));
  };
  const ask = (feature: WinFeature, on: boolean, title: string, detail: string) => setPending({ title, detail, run: () => fs.winSet(feature, on) });

  return (
    <div className="modal-backdrop" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div className="modal" data-wide role="dialog" aria-modal="true" aria-label="Configuración">
        <h2>Configuración</h2>
        <div className="modal-body">
          <h3 className="set-title">Integración con Windows</h3>
          {!status ? <p>Consultando el estado…</p> : (
            <>
              {status.stale && (
                <div className="set-warn" role="status">
                  <p>Hay entradas que apuntan a otra instalación de LFind o a una versión de desarrollo. Pueden abrir ventanas de consola o páginas de error.</p>
                  <button className="btn" onClick={() => setPending({ title: "Reparar la integración", detail: "Lo que esté activado pasará a abrir esta instalación de LFind.", run: () => fs.winRepair() })}>Reparar</button>
                </div>
              )}
              <div className="set-hero">
                <div>
                  <strong>Explorador preferido</strong>
                  <p>{status.preferred ? "Las carpetas que abren otras aplicaciones se abren en LFind." : "Las carpetas se abren con el Explorador de Windows."}</p>
                </div>
                {status.preferred
                  ? <button className="btn" onClick={() => ask("preferred", false, "Dejar de usar LFind como explorador preferido", "Las carpetas volverán a abrirse como antes.")}>Dejar de usarlo</button>
                  : <button className="btn primary" onClick={() => ask("preferred", true, "Establecer LFind como explorador preferido", "Se cambia el verbo predeterminado de carpetas y unidades para tu usuario, en el Registro (HKEY_CURRENT_USER). No requiere permisos de administrador. Se guarda el valor anterior para poder restaurarlo.")}>Establecer LFind como explorador preferido</button>}
              </div>
              {ROWS.map((r) => {
                const on = status[r.key] as boolean;
                return (
                  <div className="set-row" key={r.feature}>
                    <span><strong>{r.label}</strong><em>{on ? "Activado" : "Desactivado"}</em></span>
                    <button className="btn" onClick={() => ask(r.feature, !on, `${on ? "Desactivar" : "Activar"}: ${r.label}`, on ? "Se retirará esta entrada del Registro de tu usuario." : `${r.change} Se escribe en el Registro de tu usuario.`)}>{on ? "Desactivar" : "Activar"}</button>
                  </div>
                );
              })}
              <p className="set-note"><strong>Qué no puede hacer Windows:</strong> no existe una forma oficial de sustituir todas las ventanas del Explorador. <kbd>Win</kbd>+<kbd>E</kbd>, el propio Explorador cuando ya está abierto, el escritorio y los diálogos de abrir y guardar seguirán siendo de Windows.</p>
            </>
          )}
          {pending && (
            <div className="set-confirm" role="alertdialog" aria-label="Confirmar el cambio">
              <strong>{pending.title}</strong>
              <p>{pending.detail}</p>
              <div className="ap-row">
                <button className="btn primary" disabled={busy} onClick={confirm}>Confirmar</button>
                <button className="btn" disabled={busy} onClick={() => setPending(null)}>Cancelar</button>
              </div>
            </div>
          )}
        </div>
        <div className="modal-actions">
          <button className="btn" disabled={!status} onClick={() => setPending({ title: "Restaurar configuración original", detail: "Se retira todo lo que LFind añadió al Registro y se recupera el valor anterior. Lo que no creó LFind no se toca.", run: () => fs.winRestore() })}>Restaurar configuración original</button>
          <button className="btn primary" onClick={close}>Cerrar</button>
        </div>
      </div>
    </div>
  );
}
