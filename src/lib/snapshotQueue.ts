import type { DisplayPreference } from '../hooks/useDisplayPreference';
import { isStepFilePath } from './stepFile';

/**
 * Which files the background renderer should snapshot next. In the 3D view
 * every missing snapshot is rendered. With "image" only files without an
 * image of their own (STL, OBJ, ...) are rendered - otherwise they would show
 * the placeholder until opened once, while 3MF files already bring a picture.
 * STEP files are skipped entirely when this build has no STEP preview -
 * queuing them would only produce a guaranteed failure.
 */
export function snapshotQueue(
  models: { id: string; path: string; thumbnailImage: string | null }[],
  pendingIds: string[],
  preference: DisplayPreference,
  hasStepPreview: boolean,
): string[] {
  const byId = new Map(models.map((m) => [m.id, m]));
  const renderable = pendingIds.filter((id) => {
    if (hasStepPreview) return true;
    const model = byId.get(id);
    return !model || !isStepFilePath(model.path);
  });
  if (preference === 'render') return renderable;
  const withoutImage = new Set(models.filter((m) => !m.thumbnailImage).map((m) => m.id));
  return renderable.filter((id) => withoutImage.has(id));
}
