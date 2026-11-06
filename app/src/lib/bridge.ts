// The one way the UI talks to the core: rpc(method, params). In the desktop app this is
// a Tauri command; in a browser (dev, UI tests) it is the HTTP bridge (`cutaway serve`).
import { invoke } from '@tauri-apps/api/core';

export class BridgeError extends Error {
  constructor(
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

interface ErrorBody {
  code: string;
  message: string;
}

export const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

const httpBase: string =
  (typeof location !== 'undefined' && new URLSearchParams(location.search).get('bridge')) ||
  import.meta.env.VITE_BRIDGE_URL ||
  'http://127.0.0.1:7357';

export async function call<T = unknown>(method: string, params: object = {}): Promise<T> {
  if (inTauri) {
    try {
      return await invoke<T>('rpc', { method, params });
    } catch (e) {
      const b = e as ErrorBody;
      throw new BridgeError(b?.code ?? 'error', b?.message ?? String(e));
    }
  }
  const res = await fetch(`${httpBase}/rpc`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ method, params }),
  });
  const body = (await res.json()) as { ok: boolean; result?: T; error?: ErrorBody };
  if (!body.ok) throw new BridgeError(body.error?.code ?? 'error', body.error?.message ?? 'request failed');
  return body.result as T;
}

/** Where to save a file: the native dialog in the app, a question in the browser. Null when
 *  there's no answer. */
export async function savePath(opts: { title: string; name: string; filters?: { name: string; extensions: string[] }[] }): Promise<string | null> {
  if (!inTauri) return window.prompt(opts.title, opts.name);
  const { save } = await import('@tauri-apps/plugin-dialog');
  return save({ title: opts.title, defaultPath: opts.name, filters: opts.filters });
}

/** One path from the native dialog, or in the browser a question. Null when there's no answer. */
export async function choosePath(opts: { directory?: boolean; title: string; filters?: { name: string; extensions: string[] }[] }): Promise<string | null> {
  if (!inTauri) return window.prompt(opts.title) || null;
  return (await pickPath(opts))?.[0] ?? null;
}

/** Native file dialog in the app; null in the browser (the UI offers a path field instead). */
export async function pickPath(opts: { directory?: boolean; multiple?: boolean; title?: string; filters?: { name: string; extensions: string[] }[] }): Promise<string[] | null> {
  if (!inTauri) return null;
  const { open } = await import('@tauri-apps/plugin-dialog');
  const r = await open({ directory: opts.directory, multiple: opts.multiple, title: opts.title, filters: opts.filters });
  if (r == null) return [];
  return Array.isArray(r) ? r : [r];
}
