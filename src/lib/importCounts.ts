import type { ImportCounts, ImportJobResult } from '../types';

/**
 * An archive import job reports the archives themselves; the models inside the
 * extracted archives live in the archive entries. Counting them in makes
 * "5 models imported" match what the catalog gained.
 */
export function countsWithArchiveModels(counts: ImportCounts, groups: ImportJobResult['groups'] | undefined): ImportCounts {
  let imported = counts.imported, duplicate = counts.duplicate, skipped = counts.skipped;
  for (const archive of groups?.archive ?? []) {
    if (!('models' in archive)) continue;
    imported += archive.models.imported.length;
    duplicate += archive.models.duplicates.length;
    skipped += archive.models.skipped.length;
  }
  return { ...counts, imported, duplicate, skipped };
}
