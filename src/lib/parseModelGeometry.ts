// 4-byte LE header length, space-padded JSON { meshes, palette }, then
// Float32 positions/normals and Uint32 indices, all 4-byte aligned.
// The decoder also accepts the original array header (without colors).
export interface GeometryColor { name: string; color: string }
export interface GeometryGroup { start: number; count: number; colorIndex: number | null }
export interface ParsedMesh {
  position: Float32Array;
  normal: Float32Array | null;
  index: Uint32Array;
  objectName?: string;
  groups?: GeometryGroup[];
}
export interface ModelGeometry { meshes: ParsedMesh[]; palette: GeometryColor[] }
interface MeshHeaderEntry {
  vertexCount: number;
  hasNormal: boolean;
  indexCount: number;
  objectName?: string;
  groups?: GeometryGroup[];
}
export function decodeModelGeometry(buffer: ArrayBuffer): ModelGeometry {
  const view = new DataView(buffer);
  const headerLen = view.getUint32(0, true);
  const raw = JSON.parse(new TextDecoder().decode(new Uint8Array(buffer, 4, headerLen)));
  const headers: MeshHeaderEntry[] = Array.isArray(raw) ? raw : raw.meshes;
  const palette: GeometryColor[] = Array.isArray(raw) ? [] : raw.palette;
  let offset = 4 + headerLen;
  const meshes: ParsedMesh[] = [];
  for (const header of headers) {
    const position = new Float32Array(buffer, offset, header.vertexCount * 3);
    offset += position.byteLength;
    let normal: Float32Array | null = null;
    if (header.hasNormal) {
      normal = new Float32Array(buffer, offset, header.vertexCount * 3);
      offset += normal.byteLength;
    }
    const index = new Uint32Array(buffer, offset, header.indexCount);
    offset += index.byteLength;
    meshes.push({ position, normal, index, objectName: header.objectName, groups: header.groups });
  }
  return { meshes, palette };
}
