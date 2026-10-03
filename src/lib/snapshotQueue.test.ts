import { describe, expect, it } from 'vitest';
import { snapshotQueue } from './snapshotQueue';

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
