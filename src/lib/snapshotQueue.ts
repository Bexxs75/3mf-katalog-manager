import type { DisplayPreference } from '../hooks/useDisplayPreference';
import { isStepFilePath } from './stepFile';

export const MAX_BACKGROUND_MESH_BYTES = 200 * 1024 * 1024;

export function nextRendererGeneration(
  done: number, generation: number, event: 'captured' | 'error' | 'contextlost',
): { done: number; generation: number } {
  const nextDone = event === 'captured' ? done + 1 : 0;
  return nextDone >= 200 || event !== 'captured'
    ? { done: 0, generation: generation + 1 }
    : { done: nextDone, generation };
}

const attemptPrefix = '3mf-katalog-preview-attempt:';
const activeAttempts = new Map<string, number>();

/** Persist before native parsing: an aborted process cannot run effect cleanup. */
export function beginPreviewAttempt(id: string): () => void {
  activeAttempts.set(id, (activeAttempts.get(id) ?? 0) + 1);
  try { localStorage.setItem(attemptPrefix + id, '1'); } catch { /* Storage can be unavailable. */ }
  let finished = false;
  return () => {
    if (finished) return;
    finished = true;
    const remaining = (activeAttempts.get(id) ?? 1) - 1;
    if (remaining > 0) {
      activeAttempts.set(id, remaining);
      return;
    }
    activeAttempts.delete(id);
    try { localStorage.removeItem(attemptPrefix + id); } catch { /* Storage can be unavailable. */ }
  };
}

export function hasInterruptedPreviewAttempt(id: string): boolean {
  if (activeAttempts.has(id)) return false;
  try { return localStorage.getItem(attemptPrefix + id) !== null; } catch { return false; }
}

/**
 * Which files the background renderer should snapshot next. In the 3D view
 * every missing snapshot is rendered. With "image" only files without an
 * image of their own (STL, OBJ, ...) are rendered - otherwise they would show
 * the placeholder until opened once, while 3MF files already bring a picture.
 * STEP files are skipped entirely when this build has no STEP preview -
 * queuing them would only produce a guaranteed failure.
 */
export function snapshotQueue(
  models: { id: string; path: string; thumbnailImage: string | null; hasThumbnail?: boolean; fileSizeBytes?: number }[],
  pendingIds: string[],
  preference: DisplayPreference,
  hasStepPreview: boolean,
): string[] {
  const byId = new Map(models.map((m) => [m.id, m]));
  const renderable = pendingIds.filter((id) => {
    const model = byId.get(id);
    // Uncompressed meshes can block the UI while decoding and uploading geometry.
    if (model && /\.(stl|obj|ply)$/i.test(model.path) &&
      (model.fileSizeBytes ?? 0) > MAX_BACKGROUND_MESH_BYTES) return false;
    return !model || !isStepFilePath(model.path) ||
      (hasStepPreview && !hasInterruptedPreviewAttempt(id));
  });
  if (preference === 'render') return renderable;
  const withoutImage = new Set(models.filter((m) => !(m.hasThumbnail ?? !!m.thumbnailImage)).map((m) => m.id));
  return renderable.filter((id) => withoutImage.has(id));
}
