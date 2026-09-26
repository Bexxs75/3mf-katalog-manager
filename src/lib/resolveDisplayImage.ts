import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface ImageSources {
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
}

/**
 * Picks the image source to display: an uploaded image (customImage) always
 * wins, then the "Preferred view" setting decides, with the respective
 * other automatic source as fallback.
 */
export function resolveDisplayImage(model: ImageSources, preference: DisplayPreference): string | null {
  if (model.customImage) return model.customImage;
  return preference === 'render'
    ? model.renderSnapshotImage ?? model.thumbnailImage
    : model.thumbnailImage ?? model.renderSnapshotImage;
}
