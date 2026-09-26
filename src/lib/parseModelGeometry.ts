// Decodes the binary format of get_model_geometry (`encode_render_meshes`
// in Rust) directly into typed array views on the ArrayBuffer.
//
// Wire format:
// - 4 bytes: length of the JSON header, little-endian u32
// - JSON header (padded with spaces to a multiple of 4 bytes):
//   array of { vertexCount, hasNormal, indexCount }
// - per mesh, in header order: Float32 positions, optionally
//   Float32 normals, then always Uint32 indices. Every section consists
//   only of 4-byte elements, so the offset between meshes stays
//   aligned automatically.

export interface ParsedMesh {
  position: Float32Array;
  normal: Float32Array | null;
  index: Uint32Array;
}

interface MeshHeaderEntry {
  vertexCount: number;
  hasNormal: boolean;
  indexCount: number;
}

export function decodeModelGeometry(buffer: ArrayBuffer): ParsedMesh[] {
  const view = new DataView(buffer);
  const headerLen = view.getUint32(0, true);
  const headerBytes = new Uint8Array(buffer, 4, headerLen);
  const headers: MeshHeaderEntry[] = JSON.parse(new TextDecoder().decode(headerBytes));

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

    meshes.push({ position, normal, index });
  }

  return meshes;
}
