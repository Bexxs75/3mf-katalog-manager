import { invoke } from './transport';
import { getSnapshot, subscribe } from './webSession';

// Bound both concurrent bodies and retained decoded-image strings (up to 5 MiB each).
const cache = new Map<string, string | null>();
const pending = new Map<string, Promise<string | null>>();
const waiting: (() => void)[] = [];
let active = 0;
let generation = 0;
let revision = 0;
const listeners = new Set<() => void>();
export const subscribeSpoolImages = (fn: () => void) => { listeners.add(fn); return () => { listeners.delete(fn); }; };
export const spoolImageRevision = () => revision;
export function invalidateSpoolImage(id?: string) {
  generation++;
  if (id) cache.delete(id); else cache.clear();
  pending.clear();
  revision++;
  listeners.forEach(fn => fn());
}
subscribe(() => { if (!getSnapshot().authenticated) invalidateSpoolImage(); });

/** Called only by visible image components. Recheck visibility after waiting for a slot. */
export async function loadSpoolImage(id: string, visible: () => boolean): Promise<string | null | undefined> {
  if (cache.has(id)) return cache.get(id)!;
  if (active >= 4) await new Promise<void>(resolve => waiting.push(resolve));
  else active++;
  try {
    if (!visible()) return undefined;
    if (cache.has(id)) return cache.get(id)!;
    const epoch = generation;
    let request = pending.get(id);
    if (!request) {
      request = invoke<{ imagePng: string | null }>('get_filament_spool_image', { id }).then(result => {
        if (generation === epoch) {
          cache.set(id, result.imagePng);
          while (cache.size > 32) cache.delete(cache.keys().next().value!);
        }
        return result.imagePng;
      });
      pending.set(id, request);
      void request.finally(() => { if (pending.get(id) === request) pending.delete(id); }).catch(() => {});
    }
    const result = await request;
    return generation === epoch ? result : undefined;
  } finally {
    const next = waiting.shift();
    if (next) next(); else active--;
  }
}
