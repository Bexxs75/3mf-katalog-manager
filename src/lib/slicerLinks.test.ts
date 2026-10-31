import { expect, it } from 'vitest';
import { buildSlicerUrl, fileTypeOfName, slicersFor, WEB_SLICERS } from './slicerLinks';

it('encodes the whole download URL once behind the scheme', () => {
  const slicer = WEB_SLICERS.find(s => s.id === 'orca')!;
  expect(buildSlicerUrl(slicer, 'https://katalog.example:8443', '/dl/7/9.abcd.sig/a+b.stl'))
    .toBe('orcaslicer://open?file=https%3A%2F%2Fkatalog.example%3A8443%2Fdl%2F7%2F9.abcd.sig%2Fa%2Bb.stl');
});

it('maps file names to catalog types', () => {
  expect(fileTypeOfName('A.STL')).toBe('stl');
  expect(fileTypeOfName('x.step')).toBe('stp');
  expect(fileTypeOfName('x.stp')).toBe('stp');
  expect(fileTypeOfName('x.zip')).toBeNull();
  expect(fileTypeOfName('noext')).toBeNull();
});

it('limits Bambu Studio to 3MF and Cura to formats it opens', () => {
  expect(slicersFor('3mf').map(s => s.id)).toContain('bambu');
  for (const type of ['stl', 'obj', 'stp']) expect(slicersFor(type).map(s => s.id)).not.toContain('bambu');
  expect(slicersFor('stp').map(s => s.id)).not.toContain('cura');
  expect(slicersFor('xyz')).toEqual([]);
});
