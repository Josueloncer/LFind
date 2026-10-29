import { handleSortRequest } from "./sortCore";
import type { SortRequest } from "./sortCore";

const ctx = self as unknown as {
  onmessage: ((e: MessageEvent<SortRequest>) => void) | null;
  postMessage(msg: unknown, transfer: Transferable[]): void;
};

ctx.onmessage = (e) => {
  const perm = handleSortRequest(e.data);
  ctx.postMessage({ id: e.data.id, perm }, [perm.buffer as ArrayBuffer]);
};
