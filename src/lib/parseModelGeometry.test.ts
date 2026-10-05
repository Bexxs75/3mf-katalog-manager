import { describe, expect, it } from 'vitest';
import { decodeModelGeometry } from './parseModelGeometry';

export function geometryBuffer(header: unknown, values = [0, 0, 0, 1, 0, 0, 0, 1, 0]): ArrayBuffer {
  const json = JSON.stringify(header);
  const bytes = new TextEncoder().encode(json + ' '.repeat((4 - new TextEncoder().encode(json).length % 4) % 4));
  const buffer = new ArrayBuffer(4 + bytes.length + values.length * 4 + 12);
  new DataView(buffer).setUint32(0, bytes.length, true);
  new Uint8Array(buffer, 4, bytes.length).set(bytes);
  new Float32Array(buffer, 4 + bytes.length, values.length).set(values);
  new Uint32Array(buffer, 4 + bytes.length + values.length * 4, 3).set([0, 1, 2]);
  return buffer;
}
const mesh = { vertexCount: 3, hasNormal: false, indexCount: 3 };
describe('geometry wire format', () => {
  it('reads original array headers without colors and keeps zero-copy views', () => {
    const buffer = geometryBuffer([mesh]);
    const result = decodeModelGeometry(buffer);
    expect(result.palette).toEqual([]);
    expect(result.meshes[0].position.buffer).toBe(buffer);
    expect([...result.meshes[0].index]).toEqual([0, 1, 2]);
  });
  it('reads palette, Unicode object names and aligned material groups', () => {
    const palette = [{ name: 'Steingrau', color: '#8a8f98' }];
    const groups = [{ start: 0, count: 3, colorIndex: 0 }];
    const result = decodeModelGeometry(geometryBuffer({ meshes: [{ ...mesh, objectName: 'Töpfchen', groups }], palette }));
    expect(result.palette).toEqual(palette);
    expect(result.meshes[0].objectName).toBe('Töpfchen');
    expect(result.meshes[0].groups).toEqual(groups);
    expect([...result.meshes[0].position]).toEqual([0, 0, 0, 1, 0, 0, 0, 1, 0]);
  });
});
it('accepts optional plate metadata and missing fields from older backends', () => {
  const plates = [{ number: 1, name: null }, { number: 2, name: 'Regalplatte' }];
  const result = decodeModelGeometry(geometryBuffer({ meshes: [{ ...mesh, plate: 2 }], palette: [], plates }));
  expect(result.plates).toEqual(plates);
  expect(result.meshes[0].plate).toBe(2);
  const old = decodeModelGeometry(geometryBuffer({ meshes: [mesh], palette: [] }));
  expect(old.plates).toEqual([]);
  expect(old.meshes[0].plate).toBeUndefined();
});
