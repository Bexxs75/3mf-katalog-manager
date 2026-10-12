import { beforeEach, describe, expect, it, vi } from 'vitest';
import { beginPreviewAttempt, hasInterruptedPreviewAttempt, snapshotQueue } from './snapshotQueue';

const models = [
  { id: 'stl', path: '/cat/model.stl', thumbnailImage: null },
  { id: '3mf', path: '/cat/model.3mf', thumbnailImage: 'data:image/png;base64,AAA' },
  { id: 'obj', path: '/cat/model.obj', thumbnailImage: null },
];

const modelsWithStep = [
  ...models,
  { id: 'step', path: '/cat/model.step', thumbnailImage: null },
  { id: 'stp', path: '/cat/MODEL.STP', thumbnailImage: null },
];

describe('snapshotQueue', () => {
  it('renders every missing snapshot in the 3D view', () => {
    expect(snapshotQueue(models, ['stl', '3mf', 'obj'], 'render', true)).toEqual(['stl', '3mf', 'obj']);
  });

  it('with "image" only renders files that bring no image of their own', () => {
    expect(snapshotQueue(models, ['stl', '3mf', 'obj'], 'thumbnail', true)).toEqual(['stl', 'obj']);
  });

  it('keeps the order of the pending list', () => {
    expect(snapshotQueue(models, ['obj', 'stl'], 'thumbnail', true)).toEqual(['obj', 'stl']);
  });

  it('ignores pending ids of models that are not loaded', () => {
    expect(snapshotQueue(models, ['gone', 'stl'], 'thumbnail', true)).toEqual(['stl']);
  });

  it('with STEP support, queues STEP files like any other', () => {
    expect(snapshotQueue(modelsWithStep, ['step', 'stp', 'stl'], 'render', true)).toEqual([
      'step',
      'stp',
      'stl',
    ]);
  });

  it('without STEP support, never queues .step/.stp files (case-insensitive)', () => {
    expect(snapshotQueue(modelsWithStep, ['step', 'stp', 'stl'], 'render', false)).toEqual(['stl']);
  });

  it('without STEP support, still queues an unresolved id (fails safe, not silently)', () => {
    expect(snapshotQueue(modelsWithStep, ['gone', 'stl'], 'render', false)).toEqual(['gone', 'stl']);
  });
});


describe('persistent preview attempts', () => {
  beforeEach(() => localStorage.clear());

  it('skips an interrupted STEP attempt after restart, but permits an explicit retry', () => {
    localStorage.setItem('3mf-katalog-preview-attempt:step', '1');
    expect(snapshotQueue(modelsWithStep, ['step', 'stl'], 'render', true)).toEqual(['stl']);
    const finish = beginPreviewAttempt('step');
    expect(hasInterruptedPreviewAttempt('step')).toBe(false);
    finish();
    expect(localStorage.getItem('3mf-katalog-preview-attempt:step')).toBeNull();
    expect(snapshotQueue(modelsWithStep, ['step'], 'render', true)).toEqual(['step']);
  });

  it('persists before work and only clears after all concurrent attempts finish', () => {
    const first = beginPreviewAttempt('step');
    const second = beginPreviewAttempt('step');
    expect(localStorage.getItem('3mf-katalog-preview-attempt:step')).toBe('1');
    first();
    first();
    expect(localStorage.getItem('3mf-katalog-preview-attempt:step')).toBe('1');
    second();
    expect(localStorage.getItem('3mf-katalog-preview-attempt:step')).toBeNull();
  });

  it('tolerates unavailable browser storage', () => {
    const get = vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => { throw new Error('disabled'); });
    const set = vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => { throw new Error('disabled'); });
    const remove = vi.spyOn(Storage.prototype, 'removeItem').mockImplementation(() => { throw new Error('disabled'); });
    try {
      expect(hasInterruptedPreviewAttempt('step')).toBe(false);
      expect(() => beginPreviewAttempt('step')()).not.toThrow();
    } finally {
      get.mockRestore(); set.mockRestore(); remove.mockRestore();
    }
  });
});
