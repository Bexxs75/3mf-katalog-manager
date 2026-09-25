import { describe, expect, it } from 'vitest';
import {
  evaluateImageDrop,
  isDroppableImagePath,
  isOverDropZone,
  isWindowsPlatform,
  physicalToCss,
  toCssPosition,
} from './imageDrop';

const ZONE = { left: 100, top: 50, right: 300, bottom: 150 };

describe('physicalToCss', () => {
  it('divides physical pixels by the device pixel ratio', () => {
    expect(physicalToCss({ x: 400, y: 200 }, 2)).toEqual({ x: 200, y: 100 });
    expect(physicalToCss({ x: 125, y: 125 }, 1.25)).toEqual({ x: 100, y: 100 });
  });

  it('treats a missing or broken ratio as 1', () => {
    expect(physicalToCss({ x: 10, y: 20 }, 0)).toEqual({ x: 10, y: 20 });
    expect(physicalToCss({ x: 10, y: 20 }, Number.NaN)).toEqual({ x: 10, y: 20 });
  });
});

describe('isWindowsPlatform', () => {
  it('detects Windows from the user agent or platform string', () => {
    expect(isWindowsPlatform({ userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)', platform: 'Win32' })).toBe(true);
    expect(isWindowsPlatform({ userAgent: '', platform: 'Win32' })).toBe(true);
  });

  it('does not detect Windows on macOS or Linux', () => {
    expect(
      isWindowsPlatform({ userAgent: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit', platform: 'MacIntel' }),
    ).toBe(false);
    expect(isWindowsPlatform({ userAgent: 'Mozilla/5.0 (X11; Linux x86_64)', platform: 'Linux x86_64' })).toBe(false);
  });
});

describe('toCssPosition', () => {
  // wry 0.55: WebView2 (Windows) liefert physische Pixel, WKWebView (macOS)
  // und WebKitGTK (Linux) liefern bereits CSS-/logische Pixel.
  it('Windows: divides physical pixels by devicePixelRatio', () => {
    expect(toCssPosition({ x: 400, y: 200 }, 2, true)).toEqual({ x: 200, y: 100 });
  });

  it('macOS: uses the position as-is, devicePixelRatio is ignored', () => {
    expect(toCssPosition({ x: 400, y: 200 }, 2, false)).toEqual({ x: 400, y: 200 });
  });

  it('Linux: uses the position as-is, devicePixelRatio is ignored', () => {
    expect(toCssPosition({ x: 400, y: 200 }, 2, false)).toEqual({ x: 400, y: 200 });
  });
});

describe('isOverDropZone', () => {
  it('hit-tests physical pixels ÷ dpr on Windows', () => {
    expect(isOverDropZone({ x: 400, y: 200 }, 2, true, ZONE)).toBe(true); // 200/100 CSS
    expect(isOverDropZone({ x: 400, y: 200 }, 1, true, ZONE)).toBe(false); // 400/200 CSS
    expect(isOverDropZone({ x: 100, y: 50 }, 1, true, ZONE)).toBe(true); // Rand zaehlt
    expect(isOverDropZone({ x: 150, y: 100 }, 1, true, null)).toBe(false);
  });

  it('hit-tests the raw CSS position on macOS/Linux, dpr ignored', () => {
    expect(isOverDropZone({ x: 200, y: 100 }, 2, false, ZONE)).toBe(true); // schon CSS-Pixel, drin
    expect(isOverDropZone({ x: 400, y: 200 }, 2, false, ZONE)).toBe(false); // waere nur bei Division drin
    expect(isOverDropZone({ x: 150, y: 100 }, 1, false, null)).toBe(false);
  });
});

describe('isDroppableImagePath', () => {
  it('accepts the click-upload formats on every platform path style', () => {
    expect(isDroppableImagePath('/home/u/Spule.PNG')).toBe(true);
    expect(isDroppableImagePath('C:\\Users\\u\\spule.jpeg')).toBe(true);
    expect(isDroppableImagePath('/Users/u/spule.webp')).toBe(true);
    expect(isDroppableImagePath('/home/u/spule.jpg')).toBe(true);
  });

  it('rejects other files', () => {
    expect(isDroppableImagePath('/home/u/modell.3mf')).toBe(false);
    expect(isDroppableImagePath('/home/u/bild.gif')).toBe(false);
    expect(isDroppableImagePath('/home/u/.png')).toBe(false);
    expect(isDroppableImagePath('/home/u.png/datei')).toBe(false);
  });
});

describe('evaluateImageDrop', () => {
  it('ignores drops outside the zone, whatever they contain', () => {
    expect(evaluateImageDrop(['/a.png', '/b.png'], { x: 10, y: 10 }, 1, true, ZONE)).toEqual({ kind: 'outside' });
    expect(evaluateImageDrop(['/a.png'], { x: 150, y: 100 }, 1, true, null)).toEqual({ kind: 'outside' });
  });

  it('accepts exactly one image over the zone (Windows: physical pixels)', () => {
    expect(evaluateImageDrop(['/a.png'], { x: 300, y: 200 }, 2, true, ZONE)).toEqual({ kind: 'image', path: '/a.png' });
  });

  it('accepts exactly one image over the zone (macOS/Linux: CSS pixels already)', () => {
    expect(evaluateImageDrop(['/a.png'], { x: 150, y: 100 }, 2, false, ZONE)).toEqual({ kind: 'image', path: '/a.png' });
  });

  it('rejects several files or a non-image over the zone', () => {
    expect(evaluateImageDrop(['/a.png', '/b.png'], { x: 150, y: 100 }, 1, true, ZONE)).toEqual({ kind: 'rejected', reason: 'multiple' });
    expect(evaluateImageDrop(['/a.3mf'], { x: 150, y: 100 }, 1, true, ZONE)).toEqual({ kind: 'rejected', reason: 'not-image' });
    expect(evaluateImageDrop([], { x: 150, y: 100 }, 1, true, ZONE)).toEqual({ kind: 'rejected', reason: 'not-image' });
  });
});
