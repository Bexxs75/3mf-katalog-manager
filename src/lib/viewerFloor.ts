import * as THREE from 'three';

/** Resolve CSS Color 4 tokens through the browser's color implementation. */
export function gridColor(element: HTMLElement): THREE.Color {
  const token = getComputedStyle(element).getPropertyValue('--grid').trim();
  const canvas = document.createElement('canvas');
  canvas.width = canvas.height = 1;
  const context = canvas.getContext('2d');
  if (context && token) {
    context.fillStyle = token;
    context.fillRect(0, 0, 1, 1);
    const [r, g, b] = context.getImageData(0, 0, 1, 1).data;
    return new THREE.Color().setRGB(r / 255, g / 255, b / 255, THREE.SRGBColorSpace);
  }
  return new THREE.Color(0xaaaaaa);
}

/** One transparent plane: procedural grid and soft contact shadow, no shadow map. */
export function createViewerFloor(element: HTMLElement) {
  const material = new THREE.ShaderMaterial({
    transparent: true,
    toneMapped: false,
    depthWrite: false,
    uniforms: { gridColor: { value: gridColor(element) } },
    vertexShader: `varying vec2 floorUv;
      void main() { floorUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0); }`,
    fragmentShader: `varying vec2 floorUv;
      uniform vec3 gridColor;
      void main() {
        vec2 coord = floorUv * 24.0;
        vec2 line = abs(fract(coord - 0.5) - 0.5) / max(fwidth(coord), vec2(0.0001));
        float grid = 1.0 - min(min(line.x, line.y), 1.0);
        float radius = length((floorUv - 0.5) * 2.0);
        float fade = 1.0 - smoothstep(0.45, 1.0, radius);
        float shadow = (1.0 - smoothstep(0.0, 0.65, radius)) * 0.17;
        float alpha = grid * fade * 0.65;
        gl_FragColor = vec4(mix(vec3(0.0), gridColor, alpha / max(alpha + shadow, 0.0001)), alpha + shadow);
        #include <tonemapping_fragment>
        #include <colorspace_fragment>
      }`,
  });
  const floor = new THREE.Mesh(new THREE.PlaneGeometry(1, 1), material);
  floor.rotation.x = -Math.PI / 2;
  floor.visible = false;
  return floor;
}
