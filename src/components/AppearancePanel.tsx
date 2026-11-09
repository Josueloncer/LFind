import { useEffect, useState } from "react";
import type { ReactNode } from "react";
import { BG_SLOTS, COLOR_KEYS, DEFAULT_APPEARANCE, LIMITS, PRESETS, ZONES, ZONE_LIMITS, legibilityIssues } from "../lib/appearance";
import type { Appearance, BgSlot, ColorKey, Density, Zone, ZoneStyle } from "../lib/appearance";
import { THEMES } from "../lib/themes";
import { useBackgrounds } from "../stores/backgrounds";
import { useCustomize } from "../stores/customize";
import { usePrefs } from "../stores/prefs";
import { useUi } from "../stores/ui";
import { FontPicker } from "./FontPicker";
import { FolderGlyph } from "./Glyphs";
import { X } from "./icons";

const SLOT_LABEL: Record<BgSlot, string> = { general: "Toda la ventana", sidebar: "Barra lateral", main: "Contenido", preview: "Vista previa" };
const ZONE_LABEL: Record<Zone, string> = { sidebar: "Barra lateral", main: "Contenido", secondary: "Panel secundario", floating: "Menús y diálogos" };
const ZONE_NOTE: Partial<Record<Zone, string>> = { secondary: "Se ve al activar el doble panel.", floating: "Menús, diálogos y paleta de comandos." };
const COLOR_LABEL: Record<ColorKey, string> = { accent: "Acento", selection: "Selección", border: "Bordes", text: "Texto", interactive: "Elementos interactivos", folder: "Carpetas" };
const DENSITY: [Density, string][] = [["compact", "Compacta"], ["normal", "Normal"], ["comfortable", "Cómoda"]];

function Section({ title, children }: { title: string; children: ReactNode }) {
  return <section className="ap-section"><h3>{title}</h3>{children}</section>;
}

function Slider({ label, field, unit = "%" }: { label: string; field: keyof typeof LIMITS; unit?: string }) {
  const value = usePrefs((s) => s.appearance[field]);
  const set = usePrefs((s) => s.setAppearance);
  const [min, max] = LIMITS[field];
  return (
    <label className="ap-slider">
      <span>{label}</span>
      <input type="range" min={min} max={max} value={value} aria-label={label} onChange={(e) => set({ [field]: Number(e.target.value) } as Partial<Appearance>)} />
      <output>{value}{unit}</output>
    </label>
  );
}

function ZoneControls({ zone }: { zone: Zone }) {
  const style = usePrefs((s) => s.appearance.zones[zone]);
  const setAppearance = usePrefs((s) => s.setAppearance);
  const change = (patch: Partial<ZoneStyle>) => setAppearance({ zones: { ...usePrefs.getState().appearance.zones, [zone]: { ...style, ...patch } } });
  const [blurMin, blurMax] = ZONE_LIMITS[zone].blur;
  const [clearMin, clearMax] = ZONE_LIMITS[zone].transparency;
  const blurName = `Desenfoque de ${ZONE_LABEL[zone]}`;
  const clearName = `Transparencia de ${ZONE_LABEL[zone]}`;
  return (
    <>
      {ZONE_NOTE[zone] && <p className="ap-note">{ZONE_NOTE[zone]}</p>}
      <label className="ap-toggle"><input type="checkbox" checked={style.blurOn} aria-label={`Activar desenfoque: ${ZONE_LABEL[zone]}`} onChange={(e) => change({ blurOn: e.target.checked })} /><span>Desenfoque</span></label>
      <label className="ap-slider" data-off={style.blurOn ? undefined : true}>
        <span>Intensidad</span>
        <input type="range" min={blurMin} max={blurMax} value={style.blur} disabled={!style.blurOn} aria-label={blurName} onChange={(e) => change({ blur: Number(e.target.value) })} />
        <output>{style.blurOn ? `${style.blur} px` : "—"}</output>
      </label>
      <label className="ap-slider">
        <span>Transparencia</span>
        <input type="range" min={clearMin} max={clearMax} value={style.transparency} aria-label={clearName} onChange={(e) => change({ transparency: Number(e.target.value) })} />
        <output>{style.transparency}%</output>
      </label>
    </>
  );
}

/** Centro de personalización. Es un panel lateral sin velo: cada cambio se aplica al momento sobre la propia interfaz. */
export function AppearancePanel() {
  const open = useCustomize((s) => s.appearance);
  const close = useCustomize((s) => s.closeAppearance);
  const a = usePrefs((s) => s.appearance);
  const themes = usePrefs((s) => s.customThemes);
  const theme = usePrefs((s) => s.theme);
  const { setAppearance, resetAppearance, update, saveTheme, deleteTheme } = usePrefs.getState();
  const toast = useUi((s) => s.toast);
  const images = useBackgrounds((s) => s.baked);
  const [slot, setSlot] = useState<BgSlot>("general");
  const [zone, setZone] = useState<Zone>("sidebar");
  const [name, setName] = useState("");
  const picking = useCustomize((s) => s.picking);
  const startPick = useCustomize((s) => s.startPick);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && !useUi.getState().menu && !useCustomize.getState().picking && close();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, close]);

  // Mientras se elige una imagen, el panel se retira y deja todo el explorador libre.
  if (!open || picking) return null;
  const current = a.backgrounds[slot];
  const anyImage = BG_SLOTS.some((s) => a.backgrounds[s]);
  const background = getComputedStyle(document.documentElement).getPropertyValue("--bg-content").trim();
  const issues = /^#[0-9a-f]{6}$/i.test(background) ? legibilityIssues(a, background) : [];

  const themeFolder = getComputedStyle(document.documentElement).getPropertyValue("--cat-folder").trim();
  const removeImage = () => setAppearance({ backgrounds: { ...a.backgrounds, [slot]: null } });
  const restore = () => {
    resetAppearance();
    toast("Personalización restaurada.", "success");
  };
  const keep = () => {
    const clean = name.trim();
    if (!clean) return;
    saveTheme(clean);
    setName("");
    toast(`Tema «${clean}» guardado.`, "success");
  };

  return (
    <aside className="ap" role="dialog" aria-label="Personalización">
      <header className="ap-head">
        <h2>Personalización</h2>
        <button className="ibtn" aria-label="Cerrar" title="Cerrar (Esc)" onClick={close}><X /></button>
      </header>
      <div className="ap-body">
        <Section title="Tema">
          <div className="ap-chips">
            {THEMES.map((t) => <button key={t.id} aria-pressed={theme === t.id} onClick={() => update({ theme: t.id })}>{t.label}</button>)}
          </div>
          <h4>Estilos</h4>
          <div className="ap-chips">
            {PRESETS.map((p) => <button key={p.id} onClick={() => setAppearance({ ...DEFAULT_APPEARANCE, backgrounds: a.backgrounds, colors: a.colors, ...p.patch })}>{p.label}</button>)}
          </div>
          {themes.length > 0 && (
            <>
              <h4>Mis temas</h4>
              <div className="ap-saved">
                {themes.map((t) => (
                  <div key={t.id}>
                    <button className="ap-saved-name" onClick={() => setAppearance(t.appearance)}>{t.name}</button>
                    <button className="ibtn" aria-label={`Eliminar ${t.name}`} title="Eliminar este tema" onClick={() => deleteTheme(t.id)}><X /></button>
                  </div>
                ))}
              </div>
            </>
          )}
          <form className="ap-row" onSubmit={(e) => (e.preventDefault(), keep())}>
            <input value={name} maxLength={40} spellCheck={false} placeholder="Nombre del tema" aria-label="Nombre del tema" onChange={(e) => setName(e.target.value)} />
            <button className="btn" type="submit" disabled={!name.trim()}>Guardar actual</button>
          </form>
        </Section>

        <Section title="Fondo">
          <div className="ap-chips" role="group" aria-label="Zona">
            {BG_SLOTS.map((s) => <button key={s} aria-pressed={slot === s} data-has={a.backgrounds[s] ? true : undefined} onClick={() => setSlot(s)}>{SLOT_LABEL[s]}</button>)}
          </div>
          <div className="ap-preview" style={current && images[current] ? { backgroundImage: `url("${images[current]}")` } : undefined}>{!current && <span>Sin imagen</span>}</div>
          <div className="ap-row">
            <button className="btn" onClick={() => startPick(slot)}>Elegir imagen…</button>
            {current && <button className="btn" onClick={removeImage}>Quitar</button>}
          </div>
          {anyImage && (
            <>
              <Slider label="Desenfoque de la ventana" field="blur" unit=" px" />
              <Slider label="Intensidad" field="intensity" />
              <Slider label="Contraste" field="contrast" />
              <Slider label="Opacidad del resto" field="surface" />
              <h4>Por zona</h4>
              <div className="ap-chips" role="group" aria-label="Zona de desenfoque">
                {ZONES.map((z) => <button key={z} aria-pressed={zone === z} onClick={() => setZone(z)}>{ZONE_LABEL[z]}</button>)}
              </div>
              <ZoneControls zone={zone} />
            </>
          )}
        </Section>

        <Section title="Colores">
          {COLOR_KEYS.map((k) => (
            <div className="ap-color" key={k}>
              <label>{k === "folder" && <FolderGlyph className="ap-folder" />}<input type="color" value={a.colors[k] ?? (k === "folder" && /^#[0-9a-f]{6}$/i.test(themeFolder) ? themeFolder : "#808080")} aria-label={COLOR_LABEL[k]} onChange={(e) => setAppearance({ colors: { ...a.colors, [k]: e.target.value } })} /><span>{COLOR_LABEL[k]}</span></label>
              {a.colors[k] ? <button className="ap-link" onClick={() => setAppearance({ colors: { ...a.colors, [k]: null } })}>Usar la del tema</button> : <em>La del tema</em>}
            </div>
          ))}
          {issues.map((i) => <p className="ap-warn" key={i.key} role="status">{i.message}</p>)}
        </Section>

        <Section title="Forma y tamaños">
          <Slider label="Esquinas" field="radius" />
          <Slider label="Espaciado" field="spacing" />
          <Slider label="Iconos de la interfaz" field="iconScale" />
          <Slider label="Barra lateral" field="sidebarWidth" unit=" px" />
          <div className="ap-field"><span>Densidad</span>
            <div className="ap-chips">{DENSITY.map(([v, l]) => <button key={v} aria-pressed={a.density === v} onClick={() => setAppearance({ density: v })}>{l}</button>)}</div>
          </div>
        </Section>

        <Section title="Tipografía">
          <div className="ap-field"><span>Fuente</span><FontPicker value={a.font} onChange={(font) => setAppearance({ font })} /></div>
          <Slider label="Tamaño" field="fontScale" />
        </Section>
      </div>
      <footer className="ap-foot"><button className="btn" onClick={restore}>Restaurar valores predeterminados</button></footer>
    </aside>
  );
}
