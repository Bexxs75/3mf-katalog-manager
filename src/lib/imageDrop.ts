/** Wie der Klick-Upload (`pick_and_read_image`) und `read_dropped_image` im Backend. */
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
 * Erkennt Windows an User-Agent/Platform, ohne eine zusaetzliche Abhaengigkeit
 * (z. B. `@tauri-apps/plugin-os`) einzufuehren. Ein `nav`-Parameter macht die
 * Funktion ohne globales `navigator`-Mocking testbar.
 */
export function isWindowsPlatform(
  nav: Pick<Navigator, 'userAgent' | 'platform'> = typeof navigator === 'undefined' ? { userAgent: '', platform: '' } : navigator,
): boolean {
  return /win/i.test(nav.userAgent ?? '') || /win/i.test(nav.platform ?? '');
}

/** Tauri liefert Drop-Positionen in physischen Pixeln; getBoundingClientRect() arbeitet in CSS-Pixeln. */
export function physicalToCss(position: Point, devicePixelRatio: number): Point {
  const ratio = Number.isFinite(devicePixelRatio) && devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { x: position.x / ratio, y: position.y / ratio };
}

/**
 * Wandelt eine Tauri-Drop-Position in CSS-Pixel um. Nach dem wry-0.55-Quellcode
 * unterscheidet sich das je Backend:
 * - Windows (WebView2, `ScreenToClient`): liefert physische Pixel -> durch
 *   `devicePixelRatio` teilen.
 * - macOS (WKWebView, AppKit-Points) und Linux (WebKitGTK, unskalierte
 *   Widget-Koordinaten): liefern bereits CSS-/logische Pixel -> unveraendert
 *   uebernehmen, sonst waere die Trefferzone bei Skalierung falsch versetzt.
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

/** Reine Entscheidung fuer einen Tauri-Drop: ausserhalb, genau ein Bild, oder abgelehnt (mit Grund). */
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
