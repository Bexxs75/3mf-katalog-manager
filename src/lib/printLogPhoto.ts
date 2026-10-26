import { prepareCoverImage } from './coverImage';

export const MAX_CUSTOM_IMAGE_BYTES = 5 * 1024 * 1024;

/**
 * Same checks as `readPrintLogPhoto`, but failing with the backend's translatable
 * image error codes so the upload feedback matches the desktop picker.
 */
export async function readImageFile(file: File): Promise<string> {
  try {
    const prepared = await prepareCoverImage(file);
    return await readPrintLogPhoto(prepared as File);
  } catch (error) {
    const code = error instanceof Error && error.message === 'webPhotoTooLarge' ? 'imageUploadTooLarge'
      : error instanceof Error && error.message === 'webPhotoFormat' ? 'imageUploadUnsupported'
      : 'imageUploadUnreadable';
    throw { message: code, expected: true };
  }
}

/** Validate before allocating a base64 string; only server-supported formats. */
export async function readPrintLogPhoto(file: File): Promise<string> {
  if (file.size > MAX_CUSTOM_IMAGE_BYTES) throw new Error('webPhotoTooLarge');
  const data = new Uint8Array(await file.arrayBuffer());
  const png = data.length >= 8 && [137, 80, 78, 71, 13, 10, 26, 10].every((byte, i) => data[i] === byte);
  const jpeg = data.length >= 3 && data[0] === 255 && data[1] === 216 && data[2] === 255;
  const webp = data.length >= 12 && String.fromCharCode(...data.subarray(0, 4)) === 'RIFF' && String.fromCharCode(...data.subarray(8, 12)) === 'WEBP';
  if (!png && !jpeg && !webp) throw new Error('webPhotoFormat');
  let binary = '';
  for (let i = 0; i < data.length; i += 8192) binary += String.fromCharCode(...data.subarray(i, i + 8192));
  return btoa(binary);
}
