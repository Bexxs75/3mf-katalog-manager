import type { ModelFile } from '../types';

/**
 * What a catalog entry is for the interface: a 3D model (3MF, STL, OBJ, STEP) or a slicer output
 * (G-Code, bgcode, resin files). Slicer output has no model shape, so it gets no 3D viewer, size
 * or fit data.
 */
export type FileKind = 'model' | 'gcode' | 'resin';

/** Choice of the file type filter: one kind, or everything. */
export type FileKindFilter = 'all' | FileKind;

export const FILE_KIND_FILTERS: readonly FileKindFilter[] = ['all', 'model', 'gcode', 'resin'];

/** Ending of the text and the binary G-code format; both are one catalog type. */
export function isGcodePath(path: string): boolean {
  return /\.b?gcode$/i.test(path);
}

/** Endings of resin slicer files the catalog takes in; the Rust list (`RESIN_EXTENSIONS`) is the source. */
export const RESIN_EXTENSIONS: readonly string[] = ['ctb', 'cbddlp', 'goo', 'pwmx', 'pwms', 'sl1'];

export function isResinPath(path: string): boolean {
  const ending = /\.([^./\\]+)$/.exec(path)?.[1]?.toLowerCase();
  return ending !== undefined && RESIN_EXTENSIONS.includes(ending);
}

/**
 * The catalog's own type wins; the path only decides while a record carries none (a model loaded in
 * full has no `fileType`).
 */
export function fileKindOf(model: Pick<ModelFile, 'path' | 'fileType'>): FileKind {
  if (model.fileType !== undefined) return model.fileType === 'gcode' ? 'gcode' : model.fileType === 'resin' ? 'resin' : 'model';
  if (isGcodePath(model.path)) return 'gcode';
  return isResinPath(model.path) ? 'resin' : 'model';
}

export function isGcode(model: Pick<ModelFile, 'path' | 'fileType'>): boolean {
  return fileKindOf(model) === 'gcode';
}

export function isResin(model: Pick<ModelFile, 'path' | 'fileType'>): boolean {
  return fileKindOf(model) === 'resin';
}

/** Slicer output of any kind: no model shape, so no 3D viewer, size, fit data or "open in slicer". */
export function isSlicedOutput(model: Pick<ModelFile, 'path' | 'fileType'>): boolean {
  return fileKindOf(model) !== 'model';
}

/** Label of the type badge: the text format is "G-Code", the binary one "bgcode" (both are names, not translated). */
export function gcodeBadge(path: string): 'G-Code' | 'bgcode' {
  return /\.bgcode$/i.test(path) ? 'bgcode' : 'G-Code';
}

/** The ending in capitals ("CTB", "GOO"): the format of a resin file, a name that is not translated. */
export function resinBadge(path: string): string {
  return /\.([^./\\]+)$/.exec(path)?.[1]?.toUpperCase() ?? 'RESIN';
}

/** Label of the type badge of slicer output; empty for a model. */
export function kindBadge(model: Pick<ModelFile, 'path' | 'fileType'>): string {
  const kind = fileKindOf(model);
  return kind === 'gcode' ? gcodeBadge(model.path) : kind === 'resin' ? resinBadge(model.path) : '';
}

/** Number of entries per kind over a list; used by the filter's counts. */
export function countByKind(models: readonly Pick<ModelFile, 'path' | 'fileType'>[]): Record<FileKindFilter, number> {
  const counts: Record<FileKindFilter, number> = { all: models.length, model: 0, gcode: 0, resin: 0 };
  for (const model of models) counts[fileKindOf(model)]++;
  return counts;
}
