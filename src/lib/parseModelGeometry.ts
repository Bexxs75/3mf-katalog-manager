// Decodiert das Binaerformat von get_model_geometry (`encode_render_meshes`
// in Rust) direkt in typed-array-Views auf den ArrayBuffer.
//
// Wire-Format:
// - 4 Bytes: Laenge des JSON-Headers, little-endian u32
// - JSON-Header (mit Leerzeichen auf ein Vielfaches von 4 Bytes
//   aufgepolstert): Array von { vertexCount, hasNormal, indexCount }
// - pro Mesh, in Header-Reihenfolge: Float32-Positionen, optional
//   Float32-Normalen, dann immer Uint32-Indizes. Jeder Abschnitt besteht
//   nur aus 4-Byte-Elementen, daher bleibt der Offset zwischen Meshes
//   automatisch ausgerichtet.

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
