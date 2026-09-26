/** Like the click upload (`pick_and_read_image`) and `read_dropped_image` in the backend. */
export const DROPPABLE_IMAGE_EXTENSIONS = ['png', 'jpg', 'jpeg', 'webp'] as const;

export interface Point {
  x: number;
  y: number;
}

export interface RectLike {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export type ImageDropRejection = 'multiple' | 'not-image';

export type ImageDropResult =
  | { kind: 'outside' }
  | { kind: 'image'; path: string }
  | { kind: 'rejected'; reason: ImageDropRejection };

/**
 * Detects Windows via user agent/platform without adding an extra dependency
 * (e.g. `@tauri-apps/plugin-os`). A `nav` parameter makes the function
 * testable without mocking the global `navigator`.
 */
export function isWindowsPlatform(
  nav: Pick<Navigator, 'userAgent' | 'platform'> = typeof navigator === 'undefined' ? { userAgent: '', platform: '' } : navigator,
): boolean {
  return /win/i.test(nav.userAgent ?? '') || /win/i.test(nav.platform ?? '');
}

/** Tauri delivers drop positions in physical pixels; getBoundingClientRect() works in CSS pixels. */
export function physicalToCss(position: Point, devicePixelRatio: number): Point {
  const ratio = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { x: position.x / ratio, y: position.y / ratio };
}

/**
 * Converts a Tauri drop position into CSS pixels. According to the wry 0.55
 * source this differs per backend:
 * - Windows (WebView2, `ScreenToClient`): delivers physical pixels -> divide by
 *   `devicePixelRatio`.
 * - macOS (WKWebView, AppKit points) and Linux (WebKitGTK, unscaled
 *   widget coordinates): already deliver CSS/logical pixels -> take them
 *   unchanged, otherwise the hit zone would be offset under scaling.
 */
export function toCssPosition(position: Point, devicePixelRatio: number, isWindows: boolean): Point {
  return isWindows ? physicalToCss(position, devicePixelRatio) : position;
}

export function isPointInRect(point: Point, rect: RectLike): boolean {
  return point.x >= rect.left && point.x <= rect.right && point.y >= rect.top && point.y <= rect.bottom;
}

export function isOverDropZone(
  position: Point,
  devicePixelRatio: number,
  isWindows: boolean,
  zone: RectLike | null,
): boolean {
  return zone !== null && isPointInRect(toCssPosition(position, devicePixelRatio, isWindows), zone);
}

export function isDroppableImagePath(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? '';
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return false;
  return (DROPPABLE_IMAGE_EXTENSIONS as readonly string[]).includes(name.slice(dot + 1).toLowerCase());
}

/** Pure decision for a Tauri drop: outside, exactly one image, or rejected (with reason). */
export function evaluateImageDrop(
  paths: string[],
  position: Point,
  devicePixelRatio: number,
  isWindows: boolean,
  zone: RectLike | null,
): ImageDropResult {
  if (!isOverDropZone(position, devicePixelRatio, isWindows, zone)) return { kind: 'outside' };
  if (paths.length > 1) return { kind: 'rejected', reason: 'multiple' };
  if (paths.length === 0 || !isDroppableImagePath(paths[0])) return { kind: 'rejected', reason: 'not-image' };
  return { kind: 'image', path: paths[0] };
}
