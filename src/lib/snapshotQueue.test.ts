import { describe, expect, it } from 'vitest';
import { snapshotQueue } from './snapshotQueue';

const models = [
  { id: 'stl', thumbnailImage: null },
  { id: '3mf', thumbnailImage: 'data:image/png;base64,AAA' },
  { id: 'obj', thumbnailImage: null },
];

describe('snapshotQueue', () => {
  it('renders every missing snapshot in the 3D view', () => {
    expect(snapshotQueue(models, ['stl', '3mf', 'obj'], 'render')).toEqual(['stl', '3mf', 'obj']);
  });

  it('with "image" only renders files that bring no image of their own', () => {
    expect(snapshotQueue(models, ['stl', '3mf', 'obj'], 'thumbnail')).toEqual(['stl', 'obj']);
  });

  it('keeps the order of the pending list', () => {
    expect(snapshotQueue(models, ['obj', 'stl'], 'thumbnail')).toEqual(['obj', 'stl']);
  });

  it('ignores pending ids of models that are not loaded', () => {
    expect(snapshotQueue(models, ['gone', 'stl'], 'thumbnail')).toEqual(['stl']);
  });
});
