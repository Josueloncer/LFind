import { beforeEach, describe, expect, it } from "vitest";
import { selectActiveTab, useNav } from "./nav";

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
