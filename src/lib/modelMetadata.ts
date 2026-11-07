import type { ModelFile } from '../types';
import type { Language, Translations } from '../i18n/types';
import { isResin, isSlicedOutput } from './fileKind';
import { formatBytes, formatDate, formatDimensions, formatVolumeCm3, formatWeightG } from '../i18n/format';

export type TFunction = <K extends keyof Translations>(key: K) => Translations[K];

export function buildMetaRows(
  model: ModelFile,
  t: TFunction,
  language: Language,
): { label: string; value: string }[] {
  const materialsValue =
    model.materials.length === 0
      ? t('noValue')
      : model.materials.map((m) => m.name).join(', ');
  const objectCountValue = model.objectCount === null ? t('noValue') : String(model.objectCount);
  const weightValue =
    model.estimatedWeightG === null
      ? t('noValue')
      : model.weightSource === 'slicer'
        ? formatWeightG(model.estimatedWeightG, language)
        : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;
  const weightLabel = model.weightSource === 'slicer' ? t('metaWeightFromSlicer') : t('metaWeight');

  // Slicer output has no model shape: size, volume and object count would only ever be empty.
  const sliced = isSlicedOutput(model);
  // A resin file is only catalogued: its content is not read, so there is no weight to show either.
  const resin = isResin(model);
  const plates = model.plateDimensions;
  const largest = plates && plates.length > 1
    ? plates.reduce((a, b) => a.dimensionsMm.reduce((v, n) => v * n, 1) >= b.dimensionsMm.reduce((v, n) => v * n, 1) ? a : b) : null;
  const rows = [
    ...(sliced ? [] : [
      { label: largest ? t('plateSizeLargest') : t('metaDimensions'), value: largest ? t('plateSizeHint').replace('{size}', formatDimensions(largest.dimensionsMm, language)) : formatDimensions(model.dimensionsMm, language) },
      { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    ]),
    ...(resin ? [] : [{ label: weightLabel, value: weightValue }]),
    ...(sliced ? [] : [{ label: t('metaObjectCount'), value: objectCountValue }]),
  ];

  if (model.plateCount != null && !sliced) {
    rows.push({ label: t('metaPlateCount'), value: String(model.plateCount) });
  }

  rows.push(
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  );

  return rows;
}
