import { useMemo, useState } from "react";
import { canonicalKey } from "../lib/path";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import { FOLDER_ICONS, FOLDER_ICON_CATEGORIES, folderIcon, searchFolderIcons } from "./folderIcons";
import { Pin } from "./icons";
import { Modal } from "./Modal";

/**
 * Icono de una carpeta fijada. Es solo visual: se guarda en las preferencias de LFind y la carpeta de Windows no cambia.
 * Se elige mirando la vista previa y no se guarda hasta pulsar «Aplicar».
 */
export function FolderIconDialog({ path }: { path: string }) {
  const close = useUi((s) => s.closeDialog);
  const fav = usePrefs((s) => s.favorites.find((f) => canonicalKey(f.path) === canonicalKey(path)));
  const setIcon = usePrefs((s) => s.setFavoriteIcon);
  const [chosen, setChosen] = useState<string | null>(fav?.icon && FOLDER_ICONS[fav.icon] ? fav.icon : null);
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<string | null>(null);
  const shown = useMemo(() => searchFolderIcons(query, category), [query, category]);
  if (!fav) return null;
  const Preview = folderIcon(chosen ?? undefined) ?? Pin;
  const apply = () => {
    setIcon(fav.path, chosen);
    close();
  };
  return (
    <Modal
      title="Icono de la carpeta"
      onClose={close}
      actions={
        <>
          <button className="btn fi-default" disabled={chosen === null} onClick={() => setChosen(null)}>Predeterminado</button>
          <button className="btn" onClick={close}>Cancelar</button>
          <button className="btn primary" onClick={apply}>Aplicar</button>
        </>
      }
    >
      <div className="fi-preview" aria-label="Vista previa">
        <span className="fi-preview-row"><Preview className="fi-preview-icon" aria-hidden /><span className="fi-preview-label">{fav.label}</span></span>
        <span className="fi-preview-note">{chosen ? FOLDER_ICONS[chosen].name : "Icono predeterminado"} · solo en LFind</span>
      </div>
      <input className="fi-search" value={query} spellCheck={false} placeholder="Buscar icono (trabajo, música, código…)" aria-label="Buscar icono" onChange={(e) => setQuery(e.target.value)} autoFocus />
      <div className="fi-cats" role="tablist" aria-label="Categorías">
        <button role="tab" aria-selected={category === null} onClick={() => setCategory(null)}>Todos</button>
        {FOLDER_ICON_CATEGORIES.map((c) => <button key={c.id} role="tab" aria-selected={category === c.id} onClick={() => setCategory(c.id)}>{c.label}</button>)}
      </div>
      <div className="fi-grid" role="listbox" aria-label="Iconos">
        {shown.map((id) => {
          const { Icon, name } = FOLDER_ICONS[id];
          return (
            <button key={id} role="option" aria-selected={chosen === id} title={name} aria-label={name} onClick={() => setChosen(id)} onDoubleClick={() => (setIcon(fav.path, id), close())}>
              <Icon aria-hidden />
            </button>
          );
        })}
        {shown.length === 0 && <p className="fi-empty">Ningún icono coincide con «{query}».</p>}
      </div>
    </Modal>
  );
}
