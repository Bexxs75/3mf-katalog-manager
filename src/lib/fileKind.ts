import type { ModelFile } from '../types';

/**
 * What a catalog entry is for the interface: a 3D model (3MF, STL, OBJ, STEP) or a slicer output
 * (G-Code, bgcode). Slicer output has no model shape, so it gets no 3D viewer, size or fit data.
 */
export type FileKind = 'model' | 'gcode';

/** Choice of the file type filter: one kind, or everything. */
export type FileKindFilter = 'all' | FileKind;

export const FILE_KIND_FILTERS: readonly FileKindFilter[] = ['all', 'model', 'gcode'];

/** Ending of the text and the binary G-code format; both are one catalog type. */
export function isGcodePath(path: string): boolean {
  return /\.b?gcode$/i.test(path);
}

/**
 * The catalog's own type wins; the path only decides while a record carries none (a model loaded in
 * full has no `fileType`).
 */
export function fileKindOf(model: Pick<ModelFile, 'path' | 'fileType'>): FileKind {
  if (model.fileType !== undefined) return model.fileType === 'gcode' ? 'gcode' : 'model';
  return isGcodePath(model.path) ? 'gcode' : 'model';
}

export function isGcode(model: Pick<ModelFile, 'path' | 'fileType'>): boolean {
  return fileKindOf(model) === 'gcode';
}

/** Label of the type badge: the text format is "G-Code", the binary one "bgcode" (both are names, not translated). */
export function gcodeBadge(path: string): 'G-Code' | 'bgcode' {
  return /\.bgcode$/i.test(path) ? 'bgcode' : 'G-Code';
}

/** Number of entries per kind over a list; used by the filter's counts. */
export function countByKind(models: readonly Pick<ModelFile, 'path' | 'fileType'>[]): Record<FileKindFilter, number> {
  const counts: Record<FileKindFilter, number> = { all: models.length, model: 0, gcode: 0 };
  for (const model of models) counts[fileKindOf(model)]++;
  return counts;
}
