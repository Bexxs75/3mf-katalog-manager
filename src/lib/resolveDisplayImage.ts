import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface ImageSources {
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
}

/**
 * Waehlt die anzuzeigende Bildquelle: ein hochgeladenes Bild (customImage) hat
 * immer Vorrang, danach entscheidet die Einstellung "Bevorzugte Ansicht",
 * mit der jeweils anderen automatischen Quelle als Rueckfall.
 */
export function resolveDisplayImage(model: ImageSources, preference: DisplayPreference): string | null {
  if (model.customImage) return model.customImage;
  return preference === 'render'
    ? model.renderSnapshotImage ?? model.thumbnailImage
    : model.thumbnailImage ?? model.renderSnapshotImage;
}
