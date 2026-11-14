import { afterEach, expect, it, vi } from 'vitest';
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

const platforms = [
  { name: 'macOS Safari', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.0 Safari/605.1.15', platform: 'MacIntel', maxTouchPoints: 0, mac: true },
  { name: 'macOS Chrome', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/130.0.0.0 Safari/537.36', platform: 'MacIntel', maxTouchPoints: 0, mac: true },
  { name: 'macOS Firefox', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:132.0) Gecko/20100101 Firefox/132.0', platform: 'MacIntel', maxTouchPoints: 0, mac: true },
  { name: 'Windows', userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', platform: 'Win32', maxTouchPoints: 0, mac: false },
  { name: 'Linux', userAgent: 'Mozilla/5.0 (X11; Linux x86_64)', platform: 'Linux x86_64', maxTouchPoints: 0, mac: false },
  { name: 'iPhone', userAgent: 'Mozilla/5.0 (iPhone; CPU iPhone OS 18_0 like Mac OS X)', platform: 'iPhone', maxTouchPoints: 5, mac: false },
  { name: 'iPad', userAgent: 'Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X)', platform: 'iPad', maxTouchPoints: 5, mac: false },
  { name: 'iPad desktop mode', userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 Version/18.0 Safari/605.1.15', platform: 'MacIntel', maxTouchPoints: 5, mac: false },
];
afterEach(() => vi.unstubAllGlobals());

it.each(platforms)('uses the Bambu scheme for $name', ({ mac, ...nav }) => {
  vi.stubGlobal('navigator', nav);
  const origin = 'https://katalog.example:8443';
  const path = '/dl/7/9.abcd.sig/a%2Bb.3mf';
  expect(buildSlicerUrl(WEB_SLICERS.find(s => s.id === 'bambu')!, origin, path))
    .toBe((mac ? 'bambustudioopen://' : 'bambustudio://open?file=') + encodeURIComponent(origin + path));
});

it.each(platforms)('preserves every other slicer scheme on $name', nav => {
  vi.stubGlobal('navigator', nav);
  const prefixes = {
    orca: 'orcaslicer://open?file=', creality: 'crealityprintlink://open?file=',
    elegoo: 'elegooslicer://open?file=', cura: 'cura://open?file=',
    lychee: 'lycheeslicer://open/', prusa: 'prusaslicer://open?file=',
  };
  for (const [id, prefix] of Object.entries(prefixes)) {
    expect(buildSlicerUrl(WEB_SLICERS.find(s => s.id === id)!, 'https://example.test', '/dl/a.3mf'))
      .toBe(prefix + encodeURIComponent('https://example.test/dl/a.3mf'));
  }
});
