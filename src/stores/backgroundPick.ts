import { errMsg, fs } from "../lib/fs";
import { useCustomize } from "./customize";
import { usePrefs } from "./prefs";
import { useUi } from "./ui";

const IMAGE = /\.(png|jpe?g|jfif|webp|bmp|gif|tiff?)$/i;

export const isBackgroundImage = (path: string): boolean => IMAGE.test(path);

/**
 * Usa como fondo la imagen abierta en el explorador mientras está el modo «elegir imagen».
 * Lo que no sirve se avisa y el modo sigue activo, para poder elegir otra.
 */
export async function chooseBackground(path: string, cloud = false): Promise<void> {
  const custom = useCustomize.getState();
  const slot = custom.picking;
  if (!slot || custom.pickBusy) return;
  const { toast } = useUi.getState();
  if (!isBackgroundImage(path)) return toast("Elige una imagen PNG, JPG, WebP, BMP, GIF o TIFF.", "info");
  if (cloud) return toast("Esta imagen solo está en la nube. Descárgala primero para usarla como fondo.", "info");
  useCustomize.setState({ pickBusy: true });
  try {
    const id = await fs.backgroundImport(path);
    const prefs = usePrefs.getState();
    prefs.setAppearance({ backgrounds: { ...prefs.appearance.backgrounds, [slot]: id } });
    useCustomize.getState().cancelPick();
  } catch (e) {
    toast(errMsg(e), "error");
  } finally {
    useCustomize.setState({ pickBusy: false });
  }
}
