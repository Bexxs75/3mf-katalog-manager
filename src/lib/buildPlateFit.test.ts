import { describe, expect, it } from 'vitest';
import { checkPlateFit, modelMayFitBed, plateFitForPrinters, printerBuildVolume, usableModelSize } from './buildPlateFit';

const bed: [number, number, number] = [256, 256, 256];
const printer = (name: string, x: number | null, y: number | null, z: number | null) => ({
  name, manufacturer: null, model: null, nozzleMm: null, bedXMm: x, bedYMm: y, bedZMm: z,
});

describe('checkPlateFit', () => {
  it('fits when every axis is within the bed, including an exact match', () => {
    expect(checkPlateFit([220, 180, 140], bed)).toEqual({ status: 'fits' });
    expect(checkPlateFit([256, 256, 256], bed)).toEqual({ status: 'fits' });
  });

  it.each([
    ['width', [257, 100, 100], 'width', 257],
    ['depth', [100, 257, 100], 'depth', 257],
    ['height', [100, 100, 300], 'height', 300],
  ] as const)('reports a too large %s', (_name, dims, axis, modelMm) => {
    // Both orientations are tried; a cube-shaped bed makes the reported axis unambiguous.
    expect(checkPlateFit([...dims], bed)).toEqual({ status: 'tooLarge', excess: [{ axis, modelMm }] });
  });

  it('turns the model on the plate: width and depth may swap', () => {
    // 220 x 100 does not fit 100 x 220 as given, but fits turned by 90 degrees.
    expect(checkPlateFit([220, 100, 50], [100, 250, 100])).toEqual({ status: 'fits' });
  });

  it('never swaps the height with width or depth', () => {
    // Would fit if tipped (height 200 -> width), but tipping is not assumed.
    expect(checkPlateFit([50, 50, 200], [250, 250, 100])).toEqual({
      status: 'tooLarge', excess: [{ axis: 'height', modelMm: 200 }],
    });
  });

  it('reports the orientation with fewer overshoots and rounds the shown size up', () => {
    const fit = checkPlateFit([220.2, 100, 50], [180, 180, 180]);
    expect(fit).toEqual({ status: 'tooLarge', excess: [{ axis: 'width', modelMm: 221 }] });
  });

  it('lists every axis that is too large', () => {
    const fit = checkPlateFit([300, 300, 300], bed);
    expect(fit.status === 'tooLarge' && fit.excess.map(e => e.axis)).toEqual(['width', 'depth', 'height']);
  });

  it('ignores float noise at the limit', () => {
    expect(checkPlateFit([256.0000001, 10, 10], bed)).toEqual({ status: 'fits' });
  });

  it.each([null, undefined, [0, 10, 10], [10, 0, 10], [10, 10, 0], [Number.NaN, 1, 1], [-1, 1, 1], [1, 2]])(
    'gives no verdict for unknown size %j',
    dims => {
      expect(checkPlateFit(dims as never, bed)).toEqual({ status: 'unknown' });
    },
  );
});

describe('printerBuildVolume', () => {
  it('needs all three sizes above zero', () => {
    expect(printerBuildVolume(printer('A', 256, 256, 256))).toEqual([256, 256, 256]);
    expect(printerBuildVolume(printer('B', 256, null, 256))).toBeNull();
    expect(printerBuildVolume(printer('C', 256, 256, 0))).toBeNull();
    expect(printerBuildVolume(printer('D', null, null, null))).toBeNull();
  });
});

describe('plateFitForPrinters', () => {
  it('lists only printers with a build volume, in the given order', () => {
    const result = plateFitForPrinters([220, 180, 140], [
      printer('X1C', 256, 256, 256), printer('NoBed', null, null, null), printer('Mini', 180, 180, 180),
    ]);
    expect(result.map(r => [r.printer.name, r.fit.status])).toEqual([['X1C', 'fits'], ['Mini', 'tooLarge']]);
  });

  it('marks every printer unknown for a model without size', () => {
    expect(plateFitForPrinters(null, [printer('X1C', 256, 256, 256)]).map(r => r.fit.status)).toEqual(['unknown']);
  });
});

describe('modelMayFitBed and usableModelSize', () => {
  it('keeps models without size, hides only those that certainly do not fit', () => {
    expect(modelMayFitBed(null, [10, 10, 10])).toBe(true);
    expect(modelMayFitBed([5, 5, 5], [10, 10, 10])).toBe(true);
    expect(modelMayFitBed([50, 5, 5], [10, 10, 10])).toBe(false);
    expect(usableModelSize([0, 1, 1])).toBeNull();
  });
});
