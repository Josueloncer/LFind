import { useEffect, useRef } from "react";
import { formatDuration } from "../../lib/viewerMath";
import type { ViewerFact } from "../../lib/fs";

/** Vídeo o audio con el reproductor de WebView2. Nunca arranca solo y al cambiar de archivo libera el contenido. */
export function MediaViewer({ url, audio, onFacts, onError }: { url: string; audio: boolean; onFacts(f: ViewerFact[]): void; onError(): void }) {
  const ref = useRef<HTMLVideoElement & HTMLAudioElement>(null);

  useEffect(() => {
    const el = ref.current;
    return () => {
      if (!el) return;
      el.pause();
      el.removeAttribute("src");
      el.load();
    };
  }, [url]);

  const loaded = () => {
    const el = ref.current;
    if (!el) return;
    const f: ViewerFact[] = [];
    const d = formatDuration(el.duration);
    if (d) f.push({ label: "Duración", value: d });
    if (!audio && el.videoWidth) f.push({ label: "Resolución", value: `${el.videoWidth} × ${el.videoHeight} px` });
    onFacts(f);
  };

  const Tag = audio ? "audio" : "video";
  return (
    <div className="vw-area vw-media" data-audio={audio || undefined}>
      <Tag ref={ref} src={url} controls preload="metadata" onLoadedMetadata={loaded} onError={onError} />
    </div>
  );
}
