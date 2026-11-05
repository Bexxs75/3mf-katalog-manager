import { describe, expect, it } from 'vitest';
import { countByKind, fileKindOf, gcodeBadge, isGcodePath } from './fileKind';

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
    expect(countByKind([{ path: 'a.3mf' }, { path: 'b.gcode' }, { path: 'c.bgcode' }, { path: 'd.stl' }]))
      .toEqual({ all: 4, model: 2, gcode: 2 });
    expect(countByKind([])).toEqual({ all: 0, model: 0, gcode: 0 });
  });
});
