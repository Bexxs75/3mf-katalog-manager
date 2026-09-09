import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { invoke } from '@tauri-apps/api/core';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { parseModelGeometry } from '../lib/parseModelGeometry';
import type { ParsedMesh } from '../lib/parseModelGeometry';

interface Props {
  fileId: string;
  extension: string;
}

function frameObject(object: THREE.Object3D, camera: THREE.PerspectiveCamera, controls: OrbitControls) {
  const box = new THREE.Box3().setFromObject(object);
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());

  object.position.sub(center);

  const radius = Math.max(size.x, size.y, size.z, 1) * 0.65;
  const distance = radius / Math.sin((camera.fov * Math.PI) / 360);

  camera.position.set(distance, distance * 0.8, distance);
  camera.near = distance / 100;
  camera.far = distance * 100;
  camera.updateProjectionMatrix();

  controls.target.set(0, 0, 0);
  controls.update();
}

function disposeObject(object: THREE.Object3D) {
  object.traverse((child) => {
    if (child instanceof THREE.Mesh) {
      child.geometry.dispose();
    }
  });
}

// Baut aus den von parseModelGeometry gelieferten Rohdaten wieder eine
// THREE-Objekthierarchie auf. Jede Teilgeometrie bringt bereits ihre
// kumulierte Welt-Transformation mit, daher genuegt eine flache Gruppe
// direkter Meshes.
function buildGroup(meshes: ParsedMesh[], material: THREE.MeshStandardMaterial): THREE.Group {
  const group = new THREE.Group();
  for (const mesh of meshes) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(mesh.position, 3));
    if (mesh.normal) {
      geometry.setAttribute('normal', new THREE.BufferAttribute(mesh.normal, 3));
    }
    if (mesh.index) {
      geometry.setIndex(new THREE.BufferAttribute(mesh.index, 1));
    }
    const object = new THREE.Mesh(geometry, material);
    object.matrix.fromArray(mesh.matrix);
    object.matrixAutoUpdate = false;
    group.add(object);
  }
  group.updateMatrixWorld(true);
  return group;
}

interface ViewerContext {
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer;
  controls: OrbitControls;
  material: THREE.MeshStandardMaterial;
  currentObject: THREE.Object3D | null;
}

export function ModelViewer({ fileId, extension }: Props) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const ctxRef = useRef<ViewerContext | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');

  // Renderer/Szene/Kamera/Controls/Licht werden nur einmal beim Mounten
  // aufgebaut und beim Unmounten freigegeben - ein WebGL-Kontext-Neuaufbau
  // ist teuer und bremste bei jedem Modellwechsel spuerbar die ganze App
  // aus. Modellwechsel (zweiter Effekt unten) tauschen nur das angezeigte
  // Objekt in dieser bestehenden Szene aus.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 1000);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;

    scene.add(new THREE.AmbientLight(0xffffff, 0.65));
    const key = new THREE.DirectionalLight(0xffffff, 1.1);
    key.position.set(1, 1.4, 1);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0xffffff, 0.4);
    fill.position.set(-1, -0.4, -1);
    scene.add(fill);

    const material = new THREE.MeshStandardMaterial({
      color: 0xd7c9a8,
      roughness: 0.55,
      metalness: 0.05,
    });

    const resize = () => {
      const { clientWidth, clientHeight } = container;
      if (!clientWidth || !clientHeight) return;
      camera.aspect = clientWidth / clientHeight;
      camera.updateProjectionMatrix();
      renderer.setSize(clientWidth, clientHeight);
    };

    const resizeObserver = new ResizeObserver(resize);
    resizeObserver.observe(container);
    resize();

    let frameHandle = 0;
    const animate = () => {
      frameHandle = requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
    };
    animate();

    ctxRef.current = { scene, camera, renderer, controls, material, currentObject: null };

    return () => {
      cancelAnimationFrame(frameHandle);
      resizeObserver.disconnect();
      controls.dispose();
      if (ctxRef.current?.currentObject) {
        disposeObject(ctxRef.current.currentObject);
      }
      material.dispose();
      renderer.dispose();
      container.removeChild(renderer.domElement);
      ctxRef.current = null;
    };
  }, []);

  // Modellwechsel: laedt die neue Geometrie und ersetzt nur das Objekt in
  // der bereits bestehenden Szene, statt den ganzen Viewer neu aufzubauen.
  useEffect(() => {
    const ctx = ctxRef.current;
    if (!ctx) return;

    let cancelled = false;
    setStatus('loading');

    invoke<ArrayBuffer>('get_model_geometry', { fileId })
      .then((buffer) => {
        if (cancelled) return;
        const meshes = parseModelGeometry(extension, buffer);
        const object = buildGroup(meshes, ctx.material);

        if (ctx.currentObject) {
          ctx.scene.remove(ctx.currentObject);
          disposeObject(ctx.currentObject);
        }
        ctx.currentObject = object;
        ctx.scene.add(object);

        const container = containerRef.current;
        if (container && container.clientWidth && container.clientHeight) {
          ctx.camera.aspect = container.clientWidth / container.clientHeight;
          ctx.camera.updateProjectionMatrix();
          ctx.renderer.setSize(container.clientWidth, container.clientHeight);
        }
        frameObject(object, ctx.camera, ctx.controls);
        setStatus('ready');
      })
      .catch((err) => {
        console.error('[ModelViewer] Laden fehlgeschlagen:', err);
        if (!cancelled) setStatus('error');
      });

    return () => {
      cancelled = true;
    };
  }, [fileId, extension]);

  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="absolute inset-0" />
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          {t('loadingPreview')}
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          {t('previewUnavailable')}
        </div>
      )}
    </div>
  );
}
