import { afterEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MAX_RECENT_FAILURES } from "../lib/errorRecovery";
import { fs } from "../lib/fs";
import { useUi } from "../stores/ui";
import { ErrorBoundary, RecoveryScreen } from "./ErrorBoundary";
import type { BoundaryScope } from "./ErrorBoundary";

// El ciclo completo (hijo que falla → pantalla → Reintentar) requiere un DOM real: se comprueba en WebView2/Edge (`UPDATE06_PROGRESS.md`).
// Aquí se prueba el contrato de cada pieza.

const html = (scope: BoundaryScope, retryable = true) => renderToStaticMarkup(<RecoveryScreen scope={scope} retryable={retryable} onRetry={() => {}} />);

function boundary(scope: BoundaryScope, resetKey?: unknown) {
  const b = new ErrorBoundary({ scope, resetKey, children: "contenido" });
  const setState = vi.fn((u: unknown) => {
    const patch = typeof u === "function" ? (u as (s: typeof b.state) => object)(b.state) : (u as object);
    b.state = { ...b.state, ...patch };
  });
  b.setState = setState as unknown as typeof b.setState;
  return { b, setState };
}

const failWith = (b: ErrorBoundary, e: unknown) => {
  b.state = { ...b.state, ...ErrorBoundary.getDerivedStateFromError() };
  b.componentDidCatch(e, { componentStack: "\n  en Bomba" });
};

afterEach(() => vi.restoreAllMocks());

describe("pantalla de recuperación", () => {
  it.each(["app", "view", "panel", "viewer"] as const)("ámbito %s: se anuncia como alerta y ofrece reintentar", (scope) => {
    expect(html(scope)).toContain('role="alert"');
    expect(html(scope)).toContain("Reintentar");
  });

  it("no ofrece reintentar cuando no es seguro", () => {
    for (const s of ["app", "view", "panel"] as const) expect(html(s, false)).not.toContain("Reintentar");
    expect(html("app", false)).toContain("Recargar LFind");
    expect(html("view", false)).toContain("otra carpeta");
  });

  it("a nivel de ventana conserva una zona para mover la ventana sin marco", () => {
    expect(html("app")).toContain("data-tauri-drag-region");
    expect(html("view")).not.toContain("data-tauri-drag-region");
  });
});

describe("ErrorBoundary", () => {
  it("sin error muestra su contenido", () => {
    expect(renderToStaticMarkup(<ErrorBoundary scope="view"><b>hola</b></ErrorBoundary>)).toBe("<b>hola</b>");
  });

  it("registra el detalle técnico sin enseñárselo al usuario", () => {
    const log = vi.spyOn(fs, "logUiError").mockResolvedValue();
    const { b } = boundary("view");
    failWith(b, new TypeError("secreto-tecnico"));
    expect(log).toHaveBeenCalledWith("view", "TypeError: secreto-tecnico", expect.stringContaining("en Bomba"));
    expect(renderToStaticMarkup(<>{b.render()}</>)).not.toMatch(/secreto-tecnico|Bomba|TypeError/);
  });

  it("un fallo al registrar no agrava el error", () => {
    vi.spyOn(fs, "logUiError").mockRejectedValue("sin backend");
    const { b } = boundary("panel");
    expect(() => failWith(b, new Error("x"))).not.toThrow();
  });

  it("deja de ofrecer reintentar si el error se repite", () => {
    vi.spyOn(fs, "logUiError").mockResolvedValue();
    const { b } = boundary("view");
    for (let i = 1; i < MAX_RECENT_FAILURES; i++) failWith(b, new Error("x"));
    expect(renderToStaticMarkup(<>{b.render()}</>)).toContain("Reintentar");
    failWith(b, new Error("x"));
    expect(renderToStaticMarkup(<>{b.render()}</>)).not.toContain("Reintentar");
  });

  it("se recupera al cambiar de carpeta o elemento", () => {
    vi.spyOn(fs, "logUiError").mockResolvedValue();
    const { b, setState } = boundary("view", "a");
    failWith(b, new Error("x"));
    setState.mockClear();
    b.componentDidUpdate({ scope: "view", resetKey: "a", children: null });
    expect(setState).not.toHaveBeenCalled();
    b.componentDidUpdate({ scope: "view", resetKey: "b", children: null });
    expect(b.state.failed).toBe(false);
  });

  it("a nivel de ventana muestra la ventana oculta y, al reintentar, cierra menús y diálogos", () => {
    vi.spyOn(fs, "logUiError").mockResolvedValue();
    const show = vi.spyOn(fs, "showWindow").mockResolvedValue();
    useUi.setState({ dialog: { type: "index" }, menu: { x: 0, y: 0, items: [] } });
    const { b } = boundary("app");
    failWith(b, new Error("x"));
    expect(show).toHaveBeenCalledOnce();
    (b as unknown as { retry(): void }).retry();
    expect(b.state.failed).toBe(false);
    expect(useUi.getState().dialog).toBeNull();
    expect(useUi.getState().menu).toBeNull();
  });
});
