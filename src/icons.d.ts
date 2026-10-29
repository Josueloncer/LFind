// lucide-react no publica tipos para sus archivos individuales; todos exportan un LucideIcon por defecto.
declare module "lucide-react/dist/esm/icons/*.js" {
  import type { LucideIcon } from "lucide-react";
  const Icon: LucideIcon;
  export default Icon;
}
