// A failed WebGL context is remembered for the session so background work
// can skip all pending files without mounting another unsupported viewer.
let unavailable = false;

export function isWebGLUnavailable(): boolean {
  return unavailable;
}

export function markWebGLUnavailable(): void {
  unavailable = true;
}
