import { describe, expect, it } from "vitest";
import { basename, join, parent, samePath, segments } from "./path";

describe("path (Windows)", () => {
  it("join", () => {
    expect(join("C:\\Users", "ana")).toBe("C:\\Users\\ana");
    expect(join("C:\\", "Windows")).toBe("C:\\Windows");
  });
  it("parent", () => {
    expect(parent("C:\\Users\\ana")).toBe("C:\\Users");
    expect(parent("C:\\Users")).toBe("C:\\");
    expect(parent("C:\\")).toBeNull();
    expect(parent("\\\\srv\\share\\dir")).toBe("\\\\srv\\share");
    expect(parent("\\\\srv\\share")).toBeNull();
  });
  it("basename", () => {
    expect(basename("C:\\Users\\ana")).toBe("ana");
    expect(basename("C:\\")).toBe("C:");
  });
  it("segments", () => {
    expect(segments("C:\\Users\\ana")).toEqual([
      { label: "C:", path: "C:\\" },
      { label: "Users", path: "C:\\Users" },
      { label: "ana", path: "C:\\Users\\ana" },
    ]);
  });
  it("samePath ignora mayúsculas y barra final", () => {
    expect(samePath("C:\\Users\\Ana\\", "c:\\users\\ana")).toBe(true);
    expect(samePath("C:\\", "C:\\")).toBe(true);
    expect(samePath("C:\\a", "C:\\b")).toBe(false);
  });
});

describe("path (POSIX)", () => {
  it("parent / join / segments", () => {
    expect(parent("/home/ana")).toBe("/home");
    expect(parent("/home")).toBe("/");
    expect(parent("/")).toBeNull();
    expect(join("/", "etc")).toBe("/etc");
    expect(join("/home", "ana")).toBe("/home/ana");
    expect(segments("/home/ana").map((s) => s.path)).toEqual(["/", "/home", "/home/ana"]);
    expect(samePath("/a/b", "/a/B")).toBe(false);
  });
});
