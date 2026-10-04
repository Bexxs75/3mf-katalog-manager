import { useEffect, useSyncExternalStore } from 'react';
import { listFileImages } from '../lib/api/files';
import type { ModelImages } from '../types';

const cache = new Map<string, ModelImages>();
const inFlight = new Set<string>();
const epochs = new Map<string, number>();
const listeners = new Set<() => void>();
let version = 0;
let invalidation = 0;
let generation = 0;
function notify() { version++; listeners.forEach(listener => listener()); }
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
export function getModelImages(id: string) { return cache.get(id); }
export function invalidateModelImages(id: string) {
  cache.delete(id);
  epochs.set(id, (epochs.get(id) ?? 0) + 1);
  invalidation++;
  notify();
}
export function clearModelImages() {
  cache.clear();
  epochs.clear();
  generation++;
  invalidation++;
  notify();
}

function schedule(callback: () => void): () => void {
  if (typeof window.requestIdleCallback === 'function') {
    const handle = window.requestIdleCallback(callback, { timeout: 100 });
    return () => window.cancelIdleCallback(handle);
  }
  const handle = window.setTimeout(callback, 16);
  return () => window.clearTimeout(handle);
}

/** Shared FIFO cache. Only mounted consumers request images; outstanding batches
 * can populate the cache after unmount, but invalidated responses are discarded. */
export function useModelImages(ids: string[]): ReadonlyMap<string, ModelImages> {
  useSyncExternalStore(subscribe, () => version);
  const key = JSON.stringify(ids);
  const invalidationAtRender = invalidation;
  useEffect(() => {
    let cancelled = false;
    let cancelScheduled = () => {};
    const remaining = [...new Set(JSON.parse(key) as string[])];
    const load = async () => {
      if (cancelled) return;
      const batch: string[] = [];
      while (remaining.length && batch.length < 50) {
        const id = remaining.shift()!;
        if (!cache.has(id) && !inFlight.has(id)) batch.push(id);
      }
      if (batch.length) {
        const requestedGeneration = generation;
        const requestedEpochs = batch.map(id => epochs.get(id) ?? 0);
        batch.forEach(id => inFlight.add(id));
        try {
          const images = await listFileImages(batch);
          for (const entry of images) {
            const index = batch.indexOf(entry.id);
            if (index < 0 || requestedGeneration !== generation || requestedEpochs[index] !== (epochs.get(entry.id) ?? 0)) continue;
            cache.set(entry.id, entry);
            while (cache.size > 600) cache.delete(cache.keys().next().value!);
          }
        } catch (error) {
          console.error('[model-images] loading failed:', error);
        } finally {
          batch.forEach(id => inFlight.delete(id));
          if (requestedGeneration !== generation || batch.some((id, index) => requestedEpochs[index] !== (epochs.get(id) ?? 0))) invalidation++;
          notify();
        }
      }
      if (!cancelled && remaining.length) cancelScheduled = schedule(() => { void load(); });
    };
    cancelScheduled = schedule(() => { void load(); });
    return () => { cancelled = true; cancelScheduled(); };
  }, [key, invalidationAtRender]);
  return cache;
}
