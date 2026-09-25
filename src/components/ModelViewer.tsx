import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { invoke } from '@tauri-apps/api/core';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { decodeModelGeometry } from '../lib/parseModelGeometry';
import type { ParsedMesh } from '../lib/parseModelGeometry';

interface Props {
  fileId: string;
  needsSnapshot: boolean;
  onSnapshotCaptured: (base64: string) => void;
  onError?: () => void;
  showRotationControls?: boolean;
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

// Baut aus den Rohdaten eine flache Gruppe; die Positionen kommen aus Rust
// schon weltraum-transformiert.
function buildGroup(meshes: ParsedMesh[], material: THREE.MeshStandardMaterial): THREE.Group {
  const group = new THREE.Group();
  for (const mesh of meshes) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(mesh.position, 3));
    if (mesh.normal) {
      geometry.setAttribute('normal', new THREE.BufferAttribute(mesh.normal, 3));
    }
    geometry.setIndex(new THREE.BufferAttribute(mesh.index, 1));
    group.add(new THREE.Mesh(geometry, material));
  }
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

export function ModelViewer({ fileId, needsSnapshot, onSnapshotCaptured, onError, showRotationControls }: Props) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const ctxRef = useRef<ViewerContext | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [autoRotating, setAutoRotating] = useState(false);

  // Renderer, Szene, Kamera und Licht nur einmal aufbauen: ein neuer
  // WebGL-Kontext pro Modellwechsel bremste die App spuerbar.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 1000);
    const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
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

    // Koralle nah am App-Akzent, hebt sich in beiden Themes vom Hintergrund ab.
    const material = new THREE.MeshStandardMaterial({
      color: 0xd0603f,
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
      renderer.forceContextLoss();
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
        const meshes = decodeModelGeometry(buffer);
        const object = buildGroup(meshes, ctx.material);
        // 3MF/STL sind Z-up, OrbitControls drehen um die Welt-Y-Achse. Die feste
        // Drehung um -90 Grad auf X macht jede Azimut-Drehung zum Drehteller um die
        // stehende Achse des Modells.
        object.rotateX(-Math.PI / 2);

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

        if (needsSnapshot) {
          requestAnimationFrame(() => {
            requestAnimationFrame(() => {
              if (cancelled) return;
              try {
                const dataUrl = ctx.renderer.domElement.toDataURL('image/png');
                const base64 = dataUrl.split(',')[1];
                if (base64) onSnapshotCaptured(base64);
              } catch (err) {
                console.error('[ModelViewer] Snapshot fehlgeschlagen:', err);
                onError?.();
              }
            });
          });
        }
      })
      .catch((err) => {
        console.error('[ModelViewer] Laden fehlgeschlagen:', err);
        if (!cancelled) {
          setStatus('error');
          onError?.();
        }
      });

    return () => {
      cancelled = true;
    };
  }, [fileId]);

  // OrbitControls pausiert autoRotate beim Ziehen selbst und setzt danach fort.
  useEffect(() => {
    if (ctxRef.current) {
      ctxRef.current.controls.autoRotate = autoRotating;
    }
  }, [autoRotating]);

  // Dreht die Kamera um 15 Grad um die Hochachse. Ueber THREE.Spherical, weil
  // rotateLeft/rotateRight in OrbitControls privat sind.
  const rotateStep = (direction: 1 | -1) => {
    const ctx = ctxRef.current;
    if (!ctx) return;
    const offset = ctx.camera.position.clone().sub(ctx.controls.target);
    const spherical = new THREE.Spherical().setFromVector3(offset);
    spherical.theta += direction * (Math.PI / 12);
    const newOffset = new THREE.Vector3().setFromSpherical(spherical);
    ctx.camera.position.copy(ctx.controls.target).add(newOffset);
    ctx.camera.lookAt(ctx.controls.target);
    ctx.controls.update();
  };

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
      {showRotationControls && status === 'ready' && (
        <div className="absolute bottom-3 left-1/2 -translate-x-1/2 flex items-center gap-1.5 bg-[var(--panel-2)] border border-[var(--line)] rounded-full p-1 shadow-[var(--shadow)]">
          <button
            onClick={() => rotateStep(-1)}
            title={t('rotateLeftAria')}
            className="w-7 h-7 grid place-items-center rounded-full cursor-pointer text-[var(--ink-2)] hover:bg-[var(--line)] hover:text-[var(--ink)]"
          >
            <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M15 18l-6-6 6-6" />
            </svg>
          </button>
          <div className="w-px h-4 bg-[var(--line-strong)]" />
          <button
            onClick={() => setAutoRotating((prev) => !prev)}
            title={autoRotating ? t('pauseRotationAria') : t('playRotationAria')}
            className={`w-7 h-7 grid place-items-center rounded-full cursor-pointer ${
              autoRotating
                ? 'bg-[var(--accent)] text-[var(--accent-ink)]'
                : 'text-[var(--ink-2)] hover:bg-[var(--line)] hover:text-[var(--ink)]'
            }`}
          >
            {autoRotating ? (
              <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="currentColor">
                <rect x="6" y="5" width="4" height="14" />
                <rect x="14" y="5" width="4" height="14" />
              </svg>
            ) : (
              <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="currentColor">
                <path d="M8 5v14l11-7z" />
              </svg>
            )}
          </button>
          <div className="w-px h-4 bg-[var(--line-strong)]" />
          <button
            onClick={() => rotateStep(1)}
            title={t('rotateRightAria')}
            className="w-7 h-7 grid place-items-center rounded-full cursor-pointer text-[var(--ink-2)] hover:bg-[var(--line)] hover:text-[var(--ink)]"
          >
            <svg viewBox="0 0 24 24" className="w-3.5 h-3.5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M9 18l6-6-6-6" />
            </svg>
          </button>
        </div>
      )}
    </div>
  );
}
