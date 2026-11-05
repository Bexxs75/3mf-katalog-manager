import type { ParsedMesh } from './parseModelGeometry';

/** Most parts the backend shows one STEP file as (`step_parts::MAX_PARTS`). */
export const MAX_STEP_PARTS = 256;

/** Marks the collective part of the backend ("further parts"); see `step_parts::REST_PART_NAME`. */
export const REST_PART_NAME = 'step:rest';

export interface StepPart {
  /** Position in the mesh list, which is also the index of the `parts` scope of the STL export. */
  index: number;
  /** The collective part for bodies beyond the limit. */
  rest: boolean;
  /** Bounding box size in millimetres (x, y, z). */
  size: [number, number, number];
}

/** One entry per mesh; the list is only worth showing from two parts on. */
export function stepPartsOf(meshes: ParsedMesh[]): StepPart[] {
  return meshes.map((mesh, index) => {
    const min = [Infinity, Infinity, Infinity];
    const max = [-Infinity, -Infinity, -Infinity];
    for (let i = 0; i + 2 < mesh.position.length; i += 3) {
      for (let axis = 0; axis < 3; axis++) {
        const value = mesh.position[i + axis];
        if (value < min[axis]) min[axis] = value;
        if (value > max[axis]) max[axis] = value;
      }
    }
    const size = [0, 1, 2].map(axis => (max[axis] >= min[axis] ? max[axis] - min[axis] : 0)) as [number, number, number];
    return { index, rest: mesh.objectName === REST_PART_NAME, size };
  });
}

/** `12,5 × 8 × 3,25` with at most two decimals in the given locale. */
export function formatPartSize(size: [number, number, number], locale: string): string {
  const format = new Intl.NumberFormat(locale, { maximumFractionDigits: 2 });
  return size.map(value => format.format(value)).join(' × ');
}
