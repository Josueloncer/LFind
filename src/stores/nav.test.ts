import { beforeEach, describe, expect, it } from "vitest";
import { selectPaneTab, selectActiveTab, useNav } from "./nav";

const tab = () => selectActiveTab(useNav.getState())!;

beforeEach(() => useNav.setState({ tabs: [], activeId: null }));

describe("navegación", () => {
  it("init crea una sola pestaña y es idempotente", () => {
    useNav.getState().init("C:\\Users\\ana");
    useNav.getState().init("D:\\");
    expect(useNav.getState().tabs).toHaveLength(1);
    expect(tab().path).toBe("C:\\Users\\ana");
  });

  it("navigate, atrás, adelante y subir", () => {
    const n = useNav.getState();
    n.init("C:\\a");
    n.navigate("C:\\a\\b");
    n.navigate("C:\\a\\b\\c");
    expect(tab().back).toEqual(["C:\\a", "C:\\a\\b"]);
    useNav.getState().goBack();
    expect(tab().path).toBe("C:\\a\\b");
    expect(tab().forward).toEqual(["C:\\a\\b\\c"]);
    useNav.getState().goForward();
    expect(tab().path).toBe("C:\\a\\b\\c");
    useNav.getState().goUp();
    expect(tab().path).toBe("C:\\a\\b");
  });

  it("navegar a la misma ruta no ensucia el historial; una ruta nueva borra 'adelante'", () => {
    const n = useNav.getState();
    n.init("C:\\a");
    n.navigate("c:\\A\\");
    expect(tab().back).toHaveLength(0);
    n.navigate("C:\\b");
    useNav.getState().goBack();
    useNav.getState().navigate("C:\\z");
    expect(tab().forward).toHaveLength(0);
  });

  it("las pestañas tienen historial independiente y no se cierra la última", () => {
    const n = useNav.getState();
    n.init("C:\\a");
    n.openTab("C:\\x");
    expect(useNav.getState().tabs).toHaveLength(2);
    useNav.getState().navigate("C:\\x\\y");
    const [first] = useNav.getState().tabs;
    expect(first.path).toBe("C:\\a");
    useNav.getState().closeTab(useNav.getState().activeId!);
    expect(useNav.getState().tabs).toHaveLength(1);
    useNav.getState().closeTab(useNav.getState().activeId!);
    expect(useNav.getState().tabs).toHaveLength(1);
  });

  it("jump salta varias posiciones del historial sin duplicar ni perder entradas", () => {
    const n = useNav.getState();
    n.init("C:\\a");
    for (const p of ["C:\\b", "C:\\c", "C:\\d"]) useNav.getState().navigate(p);
    // historial: a b c [d]
    useNav.getState().jump(-1);
    expect([tab().path, tab().back, tab().forward]).toEqual(["C:\\c", ["C:\\a", "C:\\b"], ["C:\\d"]]);
    useNav.getState().jump(-2);
    expect([tab().path, tab().back, tab().forward]).toEqual(["C:\\a", [], ["C:\\b", "C:\\c", "C:\\d"]]);
    useNav.getState().jump(2);
    expect([tab().path, tab().back, tab().forward]).toEqual(["C:\\c", ["C:\\a", "C:\\b"], ["C:\\d"]]);
    useNav.getState().jump(99);
    expect(tab().path).toBe("C:\\d");
    useNav.getState().jump(-99);
    expect(tab().path).toBe("C:\\a");
    expect(tab().forward).toHaveLength(3);
  });
});

describe("doble panel", () => {
  const reset = () => useNav.setState({ tabs: [], activeId: null, other: null, side: 0 });
  const here = () => selectActiveTab(useNav.getState())?.path;
  it("cada lado tiene su ruta e historial; las acciones van al panel activo", () => {
    reset();
    const n = useNav.getState();
    n.init("C:\\a");
    n.toggleDual();
    useNav.getState().navigate("C:\\a\\b");
    useNav.getState().focusSide(1);
    expect(here()).toBe("C:\\a");
    useNav.getState().navigate("C:\\z");
    const s = useNav.getState();
    expect(selectPaneTab(0)(s)?.path).toBe("C:\\a\\b");
    expect(selectPaneTab(1)(s)?.path).toBe("C:\\z");
    expect(selectPaneTab(1)(s)?.back).toEqual(["C:\\a"]);
    useNav.getState().goBack();
    expect(selectPaneTab(1)(useNav.getState())?.path).toBe("C:\\a");
    expect(selectPaneTab(0)(useNav.getState())?.path).toBe("C:\\a\\b");
  });
  it("las pestañas son independientes", () => {
    reset();
    useNav.getState().init("C:\\a");
    useNav.getState().toggleDual();
    useNav.getState().openTab("C:\\t");
    expect(useNav.getState().tabs.length).toBe(2);
    useNav.getState().focusSide(1);
    expect(useNav.getState().tabs.length).toBe(1);
  });
  it("intercambiar cambia el contenido de cada lado y conserva el lado activo", () => {
    reset();
    useNav.getState().init("C:\\a");
    useNav.getState().toggleDual();
    useNav.getState().navigate("C:\\b");
    useNav.getState().focusSide(1);
    useNav.getState().navigate("C:\\c");
    useNav.getState().swap();
    const s = useNav.getState();
    expect(s.side).toBe(1);
    expect(selectPaneTab(0)(s)?.path).toBe("C:\\c");
    expect(selectPaneTab(1)(s)?.path).toBe("C:\\b");
  });
  it("volver a un panel conserva el activo y activar el lado actual no cambia nada", () => {
    reset();
    useNav.getState().init("C:\\a");
    useNav.getState().toggleDual();
    useNav.getState().focusSide(1);
    useNav.getState().navigate("C:\\d");
    const before = useNav.getState().tabs;
    useNav.getState().focusSide(1);
    expect(useNav.getState().tabs).toBe(before);
    useNav.getState().toggleDual();
    const s = useNav.getState();
    expect([s.other, s.side, selectActiveTab(s)?.path]).toEqual([null, 0, "C:\\d"]);
    expect(selectPaneTab(0)(s)?.path).toBe("C:\\d");
  });
});

describe("orden independiente por panel (Update 12)", () => {
  const size = { sortKey: "size", sortDir: "desc", groupBy: "none" } as const;
  const date = { sortKey: "modified", sortDir: "asc", groupBy: "none" } as const;
  const orderOf = (side: 0 | 1, p = "C:\\a") => selectPaneTab(side)(useNav.getState())?.orders?.[p.toLowerCase()];

  beforeEach(() => {
    useNav.setState({ tabs: [], activeId: null, other: null, side: 0 });
    useNav.getState().init("C:\\a");
    useNav.getState().toggleDual();
  });

  it("cada panel guarda el suyo aunque muestren la misma carpeta, sea cual sea el activo", () => {
    useNav.getState().setPaneOrder(0, "C:\\a", size);
    expect(orderOf(0)).toEqual(size);
    expect(orderOf(1)).toBeUndefined();
    useNav.getState().setPaneOrder(1, "C:\\a", date);
    expect([orderOf(0), orderOf(1)]).toEqual([size, date]);
    useNav.getState().focusSide(1);
    expect([orderOf(0), orderOf(1)]).toEqual([size, date]);
  });

  it("al intercambiar los paneles, el orden viaja con su panel", () => {
    useNav.getState().setPaneOrder(0, "C:\\a", size);
    useNav.getState().swap();
    expect([orderOf(0), orderOf(1)]).toEqual([undefined, size]);
  });

  it("al volver a un panel no queda orden propio", () => {
    useNav.getState().setPaneOrder(0, "C:\\a", size);
    useNav.getState().toggleDual();
    expect(selectPaneTab(0)(useNav.getState())?.orders).toBeUndefined();
  });
});
