import { describe, expect, it } from 'vitest';
import { formatPartSize, REST_PART_NAME, stepPartsOf } from './stepParts';
import type { ParsedMesh } from './parseModelGeometry';

const mesh = (points: number[], objectName?: string): ParsedMesh =>
  ({ position: new Float32Array(points), normal: null, index: new Uint32Array([0, 1, 2]), objectName });

describe('stepPartsOf', () => {
  it('numbers the parts by mesh position and measures each bounding box', () => {
    const parts = stepPartsOf([mesh([0, 0, 0, 10, 0, 0, 0, 5, 2]), mesh([30, 0, 0, 40, 0, 0, 30, 10, 1])]);
    expect(parts.map(part => part.index)).toEqual([0, 1]);
    expect(parts[0].size).toEqual([10, 5, 2]);
    expect(parts[1].size).toEqual([10, 10, 1]);
    expect(parts.every(part => !part.rest)).toBe(true);
  });

  it('marks only the collective part of the backend', () => {
    const parts = stepPartsOf([mesh([0, 0, 0, 1, 0, 0, 0, 1, 0]), mesh([0, 0, 0, 1, 0, 0, 0, 1, 0], REST_PART_NAME)]);
    expect(parts.map(part => part.rest)).toEqual([false, true]);
  });

  it('gives an empty mesh the size zero instead of infinities', () => {
    expect(stepPartsOf([mesh([])])[0].size).toEqual([0, 0, 0]);
  });
});

describe('formatPartSize', () => {
  it('uses the decimal sign of the language and at most two decimals', () => {
    expect(formatPartSize([12.5, 8, 3.256], 'de')).toBe('12,5 × 8 × 3,26');
    expect(formatPartSize([12.5, 8, 3.256], 'en')).toBe('12.5 × 8 × 3.26');
  });
});
