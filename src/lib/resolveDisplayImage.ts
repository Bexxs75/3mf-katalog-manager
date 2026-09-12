import type { DisplayPreference } from '../hooks/useDisplayPreference';

interface ImageSources {
  customImage: string | null;
  thumbnailImage: string | null;
  renderSnapshotImage: string | null;
}

/**
 * Waehlt aus den drei getrennt gespeicherten Bildquellen einer Datei die
 * anzuzeigende aus. Ein manuell hochgeladenes Bild (customImage) hat immer
 * Vorrang - es ist eine bewusste Nutzerentscheidung, keine automatisch
 * ermittelte Quelle. Danach entscheidet die globale "Bevorzugte Ansicht"-
 * Einstellung, welche der beiden automatischen Quellen zuerst versucht
 * wird; existiert diese nicht, faellt die Funktion auf die jeweils andere
 * zurueck, statt nichts anzuzeigen.
 */
export function resolveDisplayImage(model: ImageSources, preference: DisplayPreference): string | null {
  if (model.customImage) return model.customImage;
  return preference === 'render'
    ? model.renderSnapshotImage ?? model.thumbnailImage
    : model.thumbnailImage ?? model.renderSnapshotImage;
}
