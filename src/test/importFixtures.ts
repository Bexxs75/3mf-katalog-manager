import type { ImportJobResult, ImportProgress } from '../types';
export const importCounts = { imported: 290, importedNotPlaced: 2, duplicate: 15, skipped: 5, archive: 1, known: 313 };
export const importProgress: ImportProgress = { jobId: 'j1', state: 'importing', scanComplete: true, total: 1234, found: 1234, done: 310, inFlight: 12, counts: importCounts, current: 'Lampenschirm.3mf', elapsedMs: 261000 };
export const importResult: ImportJobResult = { jobId: 'j1', source: 'files', state: 'finished', parentJobId: null, scanComplete: true, placementRequired: true, jobError: null, counts: importCounts, groups: {
  imported: [], importedNotPlaced: [{path: '/deckel.stl', fileId: '2', reason: 'cancelled'}], duplicate: [{path: '/kopie.stl', kind: 'hash', existingFileId: '1'}], skipped: [{path: '/leer.stl', reason: 'empty'}],
  archive: [{path: '/alt.zip', state: 'pending', grantId: 'grant'}, {path: '/gut.zip', state: 'finished', extractedTo: '/k/gut', strippedRoot: null, existingSkipped: 0, unsafeSkipped: 1, blockedSkipped: 0, archiveDeleted: false, deleteError: null, error: null, models: {imported: [], skipped: [{entryPath: '../bad.stl', reason: 'unsafe'}], duplicates: [{entryPath: 'copy.stl', kind: 'hash'}]}}],
} };
