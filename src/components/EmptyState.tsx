import type { ReactNode } from "react";
import { FolderGlyph } from "./Glyphs";

export function EmptyState({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty" role="status">
      <FolderGlyph />
      <h2>{title}</h2>
      {children && <p>{children}</p>}
    </div>
  );
}
