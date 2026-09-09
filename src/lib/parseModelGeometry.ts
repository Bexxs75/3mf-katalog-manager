// Extrahiert aus einer 3MF/STL-Datei flache, wiederverwendbare Mesh-Daten
// (Position/Normal/Index/Welt-Transformation) statt der three.js-Objekte
// direkt, damit der Aufrufer (ModelViewer) sie unabhaengig von den
// three.js-Loadern zu einer THREE.Group zusammenbauen kann.
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js';
import { ThreeMFLoader } from 'three/examples/jsm/loaders/3MFLoader.js';
import * as THREE from 'three';

export interface ParsedMesh {
  position: Float32Array;
  normal: Float32Array | null;
  index: Uint32Array | Uint16Array | null;
  matrix: number[];
}

function extractMesh(geometry: THREE.BufferGeometry, matrix: THREE.Matrix4): ParsedMesh {
  const position = geometry.attributes.position.array as Float32Array;
  const normal = geometry.attributes.normal ? (geometry.attributes.normal.array as Float32Array) : null;
  const index = geometry.index ? (geometry.index.array as Uint32Array | Uint16Array) : null;
  return { position, normal, index, matrix: matrix.toArray() };
}

export function parseModelGeometry(extension: string, buffer: ArrayBuffer): ParsedMesh[] {
  if (extension === 'stl') {
    const geometry = new STLLoader().parse(buffer);
    geometry.computeVertexNormals();
    return [extractMesh(geometry, new THREE.Matrix4())];
  }

  if (extension === '3mf') {
    const group = new ThreeMFLoader().parse(buffer);
    group.updateMatrixWorld(true);
    const meshes: ParsedMesh[] = [];
    group.traverse((child) => {
      if (child instanceof THREE.Mesh) {
        meshes.push(extractMesh(child.geometry, child.matrixWorld));
      }
    });
    return meshes;
  }

  throw new Error(`nicht unterstütztes Format: ${extension}`);
}
