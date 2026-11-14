import { expect, it, vi } from 'vitest';
import { COVER_MAX_EDGE, COVER_TARGET_BYTES, prepareCoverImage, type ImageIo } from './coverImage';

const PNG = [137, 80, 78, 71, 13, 10, 26, 10];
const JPEG = [255, 216, 255, 224];
const file = (head: number[], size: number) => new Blob([new Uint8Array([...head, ...new Array(Math.max(0, size - head.length)).fill(0)])]);
const fakeIo = (over: Partial<ImageIo> & { width?: number; height?: number; sizes?: number[] } = {}) => {
  const sizes = [...(over.sizes ?? [200_000])];
  const io: ImageIo = {
    decode: over.decode ?? (async () => ({ width: over.width ?? 4000, height: over.height ?? 3000 })),
    hasTransparency: over.hasTransparency ?? (async () => false),
    encode: over.encode ?? vi.fn(async (_i, _w, _h, type) => new Blob([new Uint8Array(sizes.shift() ?? 100)], { type })),
  };
  return io;
};

it('sends a small picture unchanged', async () => {
  const input = file(PNG, 10_000);
  const io = fakeIo({ width: 800, height: 600 });
  expect(await prepareCoverImage(input, io)).toBe(input);
  expect(io.encode).not.toHaveBeenCalled();
  // Exactly at the edge limit still counts as small; a large file does not.
  expect(await prepareCoverImage(input, fakeIo({ width: COVER_MAX_EDGE, height: 10 }))).toBe(input);
  const heavy = file(JPEG, COVER_TARGET_BYTES + 1);
  expect(await prepareCoverImage(heavy, fakeIo({ width: 800, height: 600 }))).not.toBe(heavy);
});

it('scales a large picture to 1600 px and encodes JPEG below 1 MiB', async () => {
  const io = fakeIo({ width: 4000, height: 3000, sizes: [300_000] });
  const out = await prepareCoverImage(file(JPEG, 6_000_000), io);
  expect(out.type).toBe('image/jpeg');
  expect(out.size).toBe(300_000);
  expect(io.encode).toHaveBeenCalledWith(expect.anything(), 1600, 1200, 'image/jpeg', 0.85);
  const tall = fakeIo({ width: 1000, height: 3200, sizes: [10] });
  await prepareCoverImage(file(PNG, 2_000_000), tall);
  expect(tall.encode).toHaveBeenCalledWith(expect.anything(), 500, 1600, 'image/jpeg', 0.85);
});

it('lowers the quality until the result is small enough', async () => {
  const io = fakeIo({ sizes: [3_000_000, 1_500_000, 900_000] });
  const out = await prepareCoverImage(file(JPEG, 9_000_000), io);
  expect(out.size).toBe(900_000);
  expect(vi.mocked(io.encode).mock.calls.map(call => call[4])).toEqual([0.85, 0.7, 0.55]);
});

it('keeps transparency by writing WebP, or PNG when WebP is unavailable', async () => {
  const webp = fakeIo({ hasTransparency: async () => true, sizes: [400_000] });
  expect((await prepareCoverImage(file(PNG, 5_000_000), webp)).type).toBe('image/webp');
  const png = fakeIo({
    hasTransparency: async () => true,
    encode: vi.fn(async (_i, _w, _h, type) => (type === 'image/webp' ? null : new Blob([new Uint8Array(500_000)], { type }))),
  });
  const out = await prepareCoverImage(file(PNG, 5_000_000), png);
  expect(out.type).toBe('image/png');
  expect(vi.mocked(png.encode).mock.calls.map(call => call[3])).toEqual(['image/webp', 'image/png']);
});

it('reports unreadable, unsupported and impossible inputs instead of sending the original', async () => {
  await expect(prepareCoverImage(file(PNG, 5_000_000), fakeIo({ decode: async () => { throw new Error('bad'); } }))).rejects.toThrow('coverUnreadable');
  await expect(prepareCoverImage(file(PNG, 5_000_000), fakeIo({ encode: async () => null }))).rejects.toThrow('coverUnreadable');
  await expect(prepareCoverImage(file(PNG, 5_000_000), fakeIo({ encode: async () => { throw new Error('no canvas'); } }))).rejects.toThrow('coverUnreadable');
  await expect(prepareCoverImage(file([1, 2, 3], 100), fakeIo())).rejects.toThrow('webPhotoFormat');
  await expect(prepareCoverImage({ size: 65 * 1024 * 1024 } as Blob, fakeIo())).rejects.toThrow('webPhotoTooLarge');
  await expect(prepareCoverImage(file(PNG, 9_000_000), fakeIo({ sizes: [6_000_000, 6_000_000, 6_000_000, 6_000_000] }))).rejects.toThrow('webPhotoTooLarge');
  // Without any browser canvas support the default implementation refuses too.
  await expect(prepareCoverImage(file(PNG, 9_000_000))).rejects.toThrow('coverUnreadable');
  // Several multi-megabyte files are built here; a shared CI runner needs more than the default 5 s.
}, 30_000);
