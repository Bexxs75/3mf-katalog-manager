import { describe, expect, it } from 'vitest';
import { countByKind, fileKindOf, gcodeBadge, isGcodePath, isResinPath, isSlicedOutput, kindBadge, resinBadge, RESIN_EXTENSIONS } from './fileKind';

describe('file kind', () => {
  it('knows both G-code endings, in any case, and nothing else', () => {
    expect(['/a/x.gcode', '/a/X.GCODE', '/a/x.bgcode', 'x.BGcode'].map(isGcodePath)).toEqual([true, true, true, true]);
    // A 3MF that merely carries "gcode" in its name is still a model.
    expect(['/a/x.gcode.3mf', '/a/x.3mf', '/a/x.stl', '/a/gcode', '/a/x.gcode.bak'].map(isGcodePath)).toEqual([false, false, false, false, false]);
  });

  it('lets the catalog type win and falls back to the path only without one', () => {
    expect(fileKindOf({ path: '/a/x.gcode', fileType: 'gcode' })).toBe('gcode');
    expect(fileKindOf({ path: '/a/x.3mf', fileType: '3mf' })).toBe('model');
    expect(fileKindOf({ path: '/a/x.bgcode' })).toBe('gcode');
    expect(fileKindOf({ path: '/a/x.stl' })).toBe('model');
  });

  it('names the badge after the format', () => {
    expect(gcodeBadge('/a/x.gcode')).toBe('G-Code');
    expect(gcodeBadge('/a/x.BGCODE')).toBe('bgcode');
  });

  it('counts per kind', () => {
    expect(countByKind([{ path: 'a.3mf' }, { path: 'b.gcode' }, { path: 'c.bgcode' }, { path: 'd.stl' }, { path: 'e.ctb' }]))
      .toEqual({ all: 5, model: 2, gcode: 2, resin: 1 });
    expect(countByKind([])).toEqual({ all: 0, model: 0, gcode: 0, resin: 0 });
  });

  it('knows the resin endings, in any case, and nothing else', () => {
    expect(['/a/x.ctb', '/a/X.CTB', '/a/x.cbddlp', 'x.Goo', 'x.pwmx', 'x.pwms', 'x.sl1'].map(isResinPath)).toEqual(Array(7).fill(true));
    // Project files of resin slicers, other formats and names that merely contain an ending stay what they are.
    expect(['/a/x.lys', '/a/x.chitubox', '/a/x.voxl', '/a/ctb', '/a/x.ctb.bak', '/a/x.ctb.3mf', '/a.ctb/x.stl', '/a/x.3mf'].map(isResinPath)).toEqual(Array(8).fill(false));
    expect(RESIN_EXTENSIONS).toEqual(['ctb', 'cbddlp', 'goo', 'pwmx', 'pwms', 'sl1']);
  });

  it('treats a resin file as slicer output with the ending as its badge', () => {
    expect(fileKindOf({ path: '/a/x.ctb', fileType: 'resin' })).toBe('resin');
    expect(fileKindOf({ path: '/a/x.goo' })).toBe('resin');
    // The catalog type wins over the ending.
    expect(fileKindOf({ path: '/a/x.ctb', fileType: 'stl' })).toBe('model');
    expect(resinBadge('/a/x.ctb')).toBe('CTB');
    expect(resinBadge('/a/model.cbddlp')).toBe('CBDDLP');
    expect(kindBadge({ path: '/a/x.GOO', fileType: 'resin' })).toBe('GOO');
    expect(kindBadge({ path: '/a/x.bgcode', fileType: 'gcode' })).toBe('bgcode');
    expect(kindBadge({ path: '/a/x.stl', fileType: 'stl' })).toBe('');
    expect(['resin', 'gcode', 'stl', '3mf'].map(fileType => isSlicedOutput({ path: '/a/x', fileType }))).toEqual([true, true, false, false]);
  });
});
