import { describe, expect, it } from 'vitest';
import { countsWithArchiveModels } from './importCounts';
import type { ImportCounts, ImportJobResult } from '../types';

const base: ImportCounts = { imported: 0, importedNotPlaced: 0, duplicate: 0, skipped: 0, archive: 2, known: 2 };
const empty = { imported: [], importedNotPlaced: [], duplicate: [], skipped: [] };

describe('countsWithArchiveModels', () => {
  it('adds the models of finished archives to the job counts', () => {
    const groups = { ...empty, archive: [
      { path: 'a.zip', state: 'finished', models: { imported: [{}, {}, {}], duplicates: [{}], skipped: [] } },
      { path: 'b.zip', state: 'finished', models: { imported: [{}, {}], duplicates: [], skipped: [{}] } },
      { path: 'c.zip', state: 'pending', grantId: 'g' },
    ] } as unknown as ImportJobResult['groups'];
    expect(countsWithArchiveModels(base, groups)).toMatchObject({ imported: 5, duplicate: 1, skipped: 1, archive: 2 });
  });
  it('leaves counts untouched without archives', () => {
    expect(countsWithArchiveModels({ ...base, imported: 4 }, { ...empty, archive: [] } as unknown as ImportJobResult['groups']).imported).toBe(4);
  });
});
