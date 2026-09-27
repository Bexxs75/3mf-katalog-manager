import type { DisplayPreference } from '../hooks/useDisplayPreference';

/**
 * Which files the background renderer should snapshot next. In the 3D view
 * every missing snapshot is rendered. With "image" only files without an
 * image of their own (STL, OBJ, ...) are rendered - otherwise they would show
 * the placeholder until opened once, while 3MF files already bring a picture.
 */
export function snapshotQueue(
  models: { id: string; thumbnailImage: string | null }[],
  pendingIds: string[],
  preference: DisplayPreference,
): string[] {
  if (preference === 'render') return pendingIds;
  const withoutImage = new Set(models.filter((m) => !m.thumbnailImage).map((m) => m.id));
  return pendingIds.filter((id) => withoutImage.has(id));
}
