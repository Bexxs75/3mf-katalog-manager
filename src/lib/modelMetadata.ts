import type { ModelFile } from '../types';
import type { Language, Translations } from '../i18n/types';
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
    model.estimatedWeightG === null ? t('noValue') : `≈ ${formatWeightG(model.estimatedWeightG, language)}`;

  const rows = [
    { label: t('metaDimensions'), value: formatDimensions(model.dimensionsMm, language) },
    { label: t('metaVolume'), value: formatVolumeCm3(model.volumeCm3, language) },
    { label: t('metaWeight'), value: weightValue },
    { label: t('metaObjectCount'), value: objectCountValue },
  ];

  if (model.plateCount !== null) {
    rows.push({ label: t('metaPlateCount'), value: String(model.plateCount) });
  }

  rows.push(
    { label: t('metaMaterial'), value: materialsValue },
    { label: t('metaFileSize'), value: formatBytes(model.fileSizeBytes, language) },
    { label: t('metaImported'), value: formatDate(model.importedAt, language) },
  );

  return rows;
}
