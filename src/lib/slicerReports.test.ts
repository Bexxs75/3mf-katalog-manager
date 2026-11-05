import { describe, expect, it } from 'vitest';
import { isChoosableModel, parseSlicerReports, parseSlicerReportSetup } from './slicerReports';

describe('parseSlicerReportSetup', () => {
  it('reads a good answer', () => {
    const setup = parseSlicerReportSetup({
      enabled: true, programPath: '/opt/app', pathChanged: true, openCount: 3, lastError: 'x',
      commands: [{ slicer: 'OrcaSlicer', commandLine: '"/opt/app" --report-sliced' }, { slicer: 'Broken', commandLine: null }, { nope: 1 }],
    });
    expect(setup).toEqual({
      enabled: true, programPath: '/opt/app', pathChanged: true, openCount: 3, lastError: 'x',
      commands: [{ slicer: 'OrcaSlicer', commandLine: '"/opt/app" --report-sliced' }, { slicer: 'Broken', commandLine: null }],
    });
  });

  it('treats a damaged answer as off and empty', () => {
    for (const raw of [null, undefined, 5, 'x', [], { enabled: 'yes', commands: 'no', openCount: 'many' }]) {
      expect(parseSlicerReportSetup(raw)).toEqual({ enabled: false, programPath: null, pathChanged: false, commands: [], openCount: 0, lastError: null });
    }
  });
});

describe('parseSlicerReports', () => {
  const good = { id: '1', reportedAt: '2026-11-05T10:00:00Z', reportedPath: '/o/a.gcode', fileName: 'a.gcode', fileSizeBytes: 5, slicer: 'Orca', printSeconds: 60,
    weightG: 1.5, filamentTypes: ['PLA', 3], state: 'open', isTest: false, suggestion: { fileId: '7', fileName: 'A.3mf', sure: true }, model: null, decidedAt: null };

  it('reads reports, drops damaged ones and unknown states', () => {
    const list = parseSlicerReports([good, { ...good, id: '2', state: 'weird' }, { id: '3' }, null, 'x', { ...good, id: '4', suggestion: { fileId: 7 } }]);
    expect(list.map(r => r.id)).toEqual(['1', '4']);
    expect(list[0].filamentTypes).toEqual(['PLA']);
    expect(list[0].suggestion).toEqual({ fileId: '7', fileName: 'A.3mf', sure: true });
    expect(list[1].suggestion).toBeNull();
  });

  it('gives an empty list for anything but a list', () => {
    expect(parseSlicerReports(undefined)).toEqual([]);
    expect(parseSlicerReports({})).toEqual([]);
  });
});

describe('isChoosableModel', () => {
  it('leaves out G-code and resin entries', () => {
    expect(['3mf', 'stl', 'step', undefined].every(isChoosableModel)).toBe(true);
    expect(isChoosableModel('gcode')).toBe(false);
    expect(isChoosableModel('resin')).toBe(false);
  });
});
