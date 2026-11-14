import { describe, expect, it } from 'vitest';
import { modelFileType, stlEntryState } from './stlExport';

const base = { readOnly: false, container: false, hasStepPreview: true };

describe('stlEntryState', () => {
  it('offers 3MF, OBJ and STEP on the desktop', () => {
    expect(stlEntryState({ ...base, fileType: '3mf' })).toBe('enabled');
    expect(stlEntryState({ ...base, fileType: 'obj' })).toBe('enabled');
    expect(stlEntryState({ ...base, fileType: 'stp' })).toBe('enabled');
  });

  it('has nothing to convert for STL and unknown types', () => {
    expect(stlEntryState({ ...base, fileType: 'stl' })).toBe('hidden');
    expect(stlEntryState({ ...base, fileType: '' })).toBe('hidden');
    expect(stlEntryState({ ...base, fileType: 'gcode' })).toBe('hidden');
  });

  it('shows STEP disabled without a STEP build and enabled while the check is unanswered', () => {
    expect(stlEntryState({ ...base, fileType: 'stp', hasStepPreview: false })).toBe('disabled');
    expect(stlEntryState({ ...base, fileType: 'stp', hasStepPreview: null })).toBe('enabled');
    // The preview check only concerns STEP.
    expect(stlEntryState({ ...base, fileType: '3mf', hasStepPreview: false })).toBe('enabled');
  });

  it('is hidden in the web version and in a container, where no save dialog can open', () => {
    expect(stlEntryState({ ...base, fileType: '3mf', readOnly: true })).toBe('hidden');
    expect(stlEntryState({ ...base, fileType: '3mf', container: true })).toBe('hidden');
  });
});

describe('modelFileType', () => {
  it('prefers the catalog type and falls back to the ending of the path or name', () => {
    expect(modelFileType({ fileType: '3MF', path: '/x/a.stl' })).toBe('3mf');
    expect(modelFileType({ path: '/x/a.STEP' })).toBe('stp');
    expect(modelFileType({ name: 'part.obj' })).toBe('obj');
    expect(modelFileType({ name: 'part.txt' })).toBe('');
    expect(modelFileType({})).toBe('');
  });
});
