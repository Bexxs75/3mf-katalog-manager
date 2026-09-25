import { describe, expect, it } from 'vitest';
import { evaluateImageDrop, isDroppableImagePath, isOverDropZone, physicalToCss } from './imageDrop';

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

describe('isOverDropZone', () => {
  it('hit-tests in CSS pixels', () => {
    expect(isOverDropZone({ x: 400, y: 200 }, 2, ZONE)).toBe(true); // 200/100 CSS
    expect(isOverDropZone({ x: 400, y: 200 }, 1, ZONE)).toBe(false); // 400/200 CSS
    expect(isOverDropZone({ x: 100, y: 50 }, 1, ZONE)).toBe(true); // Rand zaehlt
    expect(isOverDropZone({ x: 150, y: 100 }, 1, null)).toBe(false);
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
    expect(evaluateImageDrop(['/a.png', '/b.png'], { x: 10, y: 10 }, 1, ZONE)).toEqual({ kind: 'outside' });
    expect(evaluateImageDrop(['/a.png'], { x: 150, y: 100 }, 1, null)).toEqual({ kind: 'outside' });
  });

  it('accepts exactly one image over the zone', () => {
    expect(evaluateImageDrop(['/a.png'], { x: 300, y: 200 }, 2, ZONE)).toEqual({ kind: 'image', path: '/a.png' });
  });

  it('rejects several files or a non-image over the zone', () => {
    expect(evaluateImageDrop(['/a.png', '/b.png'], { x: 150, y: 100 }, 1, ZONE)).toEqual({ kind: 'rejected', reason: 'multiple' });
    expect(evaluateImageDrop(['/a.3mf'], { x: 150, y: 100 }, 1, ZONE)).toEqual({ kind: 'rejected', reason: 'not-image' });
    expect(evaluateImageDrop([], { x: 150, y: 100 }, 1, ZONE)).toEqual({ kind: 'rejected', reason: 'not-image' });
  });
});
