/** Largest edge and byte size of a cover image sent to the web server. */
export const COVER_MAX_EDGE = 1600;
export const COVER_TARGET_BYTES = 1024 * 1024;
/** What the server accepts at most; anything still bigger after shrinking is refused. */
const SERVER_MAX_BYTES = 5 * 1024 * 1024;
/** Camera originals are fine as input, because they are shrunk before sending. */
const MAX_INPUT_BYTES = 64 * 1024 * 1024;
const QUALITIES = [0.85, 0.7, 0.55, 0.4];

export interface DecodedImage {
  width: number;
  height: number;
}

/** The browser parts, replaceable in tests (jsdom has no canvas). */
export interface ImageIo {
  decode(file: Blob): Promise<DecodedImage>;
  /** Draws the image scaled to width x height and encodes it; null if the type is not supported. */
  encode(image: DecodedImage, width: number, height: number, type: string, quality: number): Promise<Blob | null>;
  /** True when any pixel of the scaled drawing is not fully opaque. */
  hasTransparency(image: DecodedImage, width: number, height: number): Promise<boolean>;
}

async function signature(file: Blob): Promise<'png' | 'jpeg' | 'webp' | null> {
  const head = new Uint8Array(await file.slice(0, 12).arrayBuffer());
  if (head.length >= 8 && [137, 80, 78, 71, 13, 10, 26, 10].every((byte, i) => head[i] === byte)) return 'png';
  if (head.length >= 3 && head[0] === 255 && head[1] === 216 && head[2] === 255) return 'jpeg';
  if (head.length >= 12 && String.fromCharCode(...head.subarray(0, 4)) === 'RIFF' && String.fromCharCode(...head.subarray(8, 12)) === 'WEBP') return 'webp';
  return null;
}

function drawing(image: DecodedImage, width: number, height: number) {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d');
  if (!context) throw new Error('coverUnreadable');
  context.drawImage(image as unknown as CanvasImageSource, 0, 0, width, height);
  return { canvas, context };
}

export const browserImageIo: ImageIo = {
  async decode(file) {
    if (typeof createImageBitmap !== 'function') throw new Error('coverUnreadable');
    return createImageBitmap(file);
  },
  async encode(image, width, height, type, quality) {
    const { canvas } = drawing(image, width, height);
    return new Promise(resolve => canvas.toBlob(blob => resolve(blob && blob.type === type ? blob : null), type, quality));
  },
  async hasTransparency(image, width, height) {
    const { context } = drawing(image, width, height);
    const pixels = context.getImageData(0, 0, width, height).data;
    for (let i = 3; i < pixels.length; i += 4) if (pixels[i] < 255) return true;
    return false;
  },
};

/**
 * Shrinks a chosen cover image before upload: at most 1600 px on the longer edge and about
 * 1 MiB. A small image is sent as it is. Transparent PNG/WebP stay WebP/PNG, everything else
 * becomes JPEG. Failures are reported, never worked around by sending the original.
 * Throws Error('webPhotoTooLarge' | 'webPhotoFormat' | 'coverUnreadable').
 */
export async function prepareCoverImage(file: Blob, io: ImageIo = browserImageIo): Promise<Blob> {
  if (file.size > MAX_INPUT_BYTES) throw new Error('webPhotoTooLarge');
  const kind = await signature(file);
  if (!kind) throw new Error('webPhotoFormat');
  let image: DecodedImage;
  try { image = await io.decode(file); } catch { throw new Error('coverUnreadable'); }
  const { width, height } = image;
  if (!(width > 0 && height > 0)) throw new Error('coverUnreadable');
  if (file.size < COVER_TARGET_BYTES && Math.max(width, height) <= COVER_MAX_EDGE) return file;

  const scale = Math.min(1, COVER_MAX_EDGE / Math.max(width, height));
  const targetWidth = Math.max(1, Math.round(width * scale));
  const targetHeight = Math.max(1, Math.round(height * scale));
  try {
    const transparent = kind !== 'jpeg' && await io.hasTransparency(image, targetWidth, targetHeight);
    // Transparency must survive: WebP first, PNG if the browser cannot write WebP.
    const types = transparent ? ['image/webp', 'image/png'] : ['image/jpeg'];
    let best: Blob | null = null;
    for (const type of types) {
      for (const quality of QUALITIES) {
        const blob = await io.encode(image, targetWidth, targetHeight, type, quality);
        if (!blob) break;
        if (!best || blob.size < best.size) best = blob;
        if (blob.size <= COVER_TARGET_BYTES) return blob;
        // PNG ignores the quality, so one attempt is enough.
        if (type === 'image/png') break;
      }
      if (best) break;
    }
    if (!best) throw new Error('coverUnreadable');
    if (best.size > SERVER_MAX_BYTES) throw new Error('webPhotoTooLarge');
    return best;
  } catch (error) {
    if (error instanceof Error && error.message === 'webPhotoTooLarge') throw error;
    throw new Error('coverUnreadable');
  }
}
