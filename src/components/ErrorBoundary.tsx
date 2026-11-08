import { Component } from "react";
import type { ErrorInfo, ReactNode } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { canRetry, errorReport, registerFailure } from "../lib/errorRecovery";
import { fs, isTauri } from "../lib/fs";
import { useOps } from "../stores/ops";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import { TriangleAlert } from "./icons";

/** `app`: toda la ventana · `view`: el contenido principal · `panel`: el panel de vista previa · `viewer`: solo el visualizador, dentro del panel. */
export type BoundaryScope = "app" | "view" | "panel" | "viewer";

const report = (scope: BoundaryScope, error: unknown, stack?: string | null) => {
  try {
    const r = errorReport(error, stack);
    void fs.logUiError(scope, r.message, r.stack).catch(() => {});
  } catch {
    /* el registro nunca debe agravar el fallo */
  }
};

/** Lo que se puede cerrar sin perder nada; suele ser lo que provocó el error (un diálogo o un menú abierto). */
const closeTransientUi = () => useUi.setState({ dialog: null, menu: null });

/** Antes de salir se guardan las preferencias pendientes, igual que hace el botón de cerrar de la ventana. */
const saveThen = (next: () => void) => void Promise.resolve(usePrefs.getState().flush()).catch(() => {}).finally(next);
const reloadApp = () => saveThen(() => window.location.reload());
const closeApp = () => saveThen(() => void getCurrentWindow().close());

const runningJobs = () => useOps.getState().jobs.some((j) => j.status === "running");

/** Pantalla de recuperación. Nunca muestra detalles técnicos y solo depende de los tokens de diseño: `GlyphDefs` puede no estar montado. */
export function RecoveryScreen({ scope, retryable, onRetry }: { scope: BoundaryScope; retryable: boolean; onRetry(): void }) {
  const retry = retryable && <button className={`btn${scope === "app" ? " primary" : ""}`} onClick={onRetry}>Reintentar</button>;

  if (scope === "viewer") {
    return (
      <div className="preview-empty" role="alert">
        <p className="crash-panel-text">No se pudo mostrar este archivo.</p>
        {retry}
      </div>
    );
  }

  if (scope === "panel") {
    return (
      <aside className="preview" aria-label="Vista previa">
        <div className="preview-empty" role="alert">
          <p className="crash-panel-text">No se pudo mostrar la vista previa de este elemento.</p>
          {retry}
        </div>
      </aside>
    );
  }

  const body = (
    <div className="empty" role="alert">
      <TriangleAlert className="crash-icon" />
      <h2>{scope === "app" ? "LFind encontró un problema" : "No se pudo mostrar este contenido"}</h2>
      <p>
        {scope === "app" ? "La interfaz dejó de responder por un error inesperado." : "Se produjo un error inesperado al dibujar esta vista."} Un error de este tipo no afecta a tus archivos.
        {!retryable && (scope === "app" ? " El error se repite, así que conviene recargar LFind." : " El error se repite: prueba a abrir otra carpeta.")}
      </p>
      {scope === "app" ? (
        <>
          <div className="crash-actions">
            {retry}
            <button className={`btn${retryable ? "" : " primary"}`} onClick={reloadApp}>Recargar LFind</button>
            {isTauri && <button className="btn" onClick={closeApp}>Cerrar</button>}
          </div>
          {runningJobs() && <p>Hay operaciones en curso: si recargas, LFind dejará de seguir su progreso.</p>}
        </>
      ) : (
        retry && <div className="crash-actions">{retry}</div>
      )}
    </div>
  );

  // Sin la barra de título, esta franja permite seguir moviendo la ventana (no tiene marco del sistema).
  return scope === "app" ? <div className="crash-app"><div className="crash-drag" data-tauri-drag-region />{body}</div> : body;
}

interface Props {
  scope: BoundaryScope;
  /** Si cambia mientras se muestra el error (otra carpeta, otro elemento), se vuelve a intentar sin más. */
  resetKey?: unknown;
  children: ReactNode;
}

interface State {
  failed: boolean;
  failures: number[];
}

/**
 * Evita que un error de renderizado deje la ventana en blanco. Solo captura errores de render: los de eventos y promesas no
 * rompen la interfaz y ya se informan con `toast`; un pánico de Rust queda fuera de React (ver `nova_core::diagnostics`).
 */
export class ErrorBoundary extends Component<Props, State> {
  state: State = { failed: false, failures: [] };

  static getDerivedStateFromError(): Partial<State> {
    return { failed: true };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    this.setState((s) => ({ failures: registerFailure(s.failures, Date.now()) }));
    report(this.props.scope, error, info.componentStack);
    // Si falla el primer render, `App` no llegó a pedir que se muestre la ventana (nace oculta).
    if (this.props.scope === "app") void fs.showWindow().catch(() => {});
  }

  componentDidUpdate(prev: Props) {
    if (this.state.failed && prev.resetKey !== this.props.resetKey) this.setState({ failed: false });
  }

  private retry = () => {
    if (this.props.scope === "app") closeTransientUi();
    this.setState({ failed: false });
  };

  render() {
    if (!this.state.failed) return this.props.children;
    return <RecoveryScreen scope={this.props.scope} retryable={canRetry(this.state.failures)} onRetry={this.retry} />;
  }
}
