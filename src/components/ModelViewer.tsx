import { Icon } from './Icon';
import { PlateSelector } from './PlateSelector';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { invoke } from '@tauri-apps/api/core';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { decodeModelGeometry } from '../lib/parseModelGeometry';
import type { ParsedMesh, GeometryColor, GeometryPlate } from '../lib/parseModelGeometry';
import { toAppError, type AppError } from '../lib/errors';
import { ViewerErrorCard, type ViewerActions } from './ViewerErrorCard';
import { createViewerFloor, gridColor } from '../lib/viewerFloor';

interface Props extends ViewerActions {
  surfaceClassName?: string;
  fileId: string;
  needsSnapshot: boolean;
  onSnapshotCaptured: (base64: string) => void;
  onError?: () => void;
  showRotationControls?: boolean;
}

function visibleBounds(object: THREE.Object3D) {
  object.updateWorldMatrix(true, true);
  const box = new THREE.Box3();
  object.traverseVisible(child => {
    if (child instanceof THREE.Mesh) box.union(new THREE.Box3().setFromObject(child));
  });
  return box;
}

function frameObject(object: THREE.Object3D, camera: THREE.PerspectiveCamera, controls: OrbitControls) {
  const box = visibleBounds(object);
  if (box.isEmpty()) return;
  const size = box.getSize(new THREE.Vector3());
  const center = box.getCenter(new THREE.Vector3());

  object.position.sub(center);

  const radius = Math.max(size.x, size.y, size.z, 1) * 0.65;
  const halfFov = (camera.fov * Math.PI) / 360;
  const distance = radius / Math.sin(Math.min(halfFov, Math.atan(Math.tan(halfFov) * camera.aspect)));

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

// Builds a flat group from the raw data; the positions come from Rust
// already transformed into world space.
function buildGroup(meshes: ParsedMesh[], material: THREE.MeshStandardMaterial, colors: THREE.MeshStandardMaterial[] = []): THREE.Group {
  const group = new THREE.Group();
  for (const mesh of meshes) {
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute('position', new THREE.BufferAttribute(mesh.position, 3));
    if (mesh.normal) {
      geometry.setAttribute('normal', new THREE.BufferAttribute(mesh.normal, 3));
    }
    geometry.setIndex(new THREE.BufferAttribute(mesh.index, 1));
    for (const range of mesh.groups ?? []) {
      geometry.addGroup(range.start, range.count, range.colorIndex === null ? 0 : range.colorIndex + 1);
    }
    const fileMaterials = mesh.groups?.length ? [material, ...colors] : material;
    const child = new THREE.Mesh(geometry, fileMaterials);
    child.userData.fileMaterials = fileMaterials;
    child.userData.plate = mesh.plate;
    group.add(child);
  }
  return group;
}

// Without WebGL (VMs without GPU passthrough, some remote desktops, blocked
// GPU drivers) creating the renderer throws. Remembered per session so the
// background snapshot queue fails each file immediately instead of trying a
// new context for every model.
let webglUnavailable = false;

interface ViewerContext {
  scene: THREE.Scene;
  camera: THREE.PerspectiveCamera;
  renderer: THREE.WebGLRenderer;
  controls: OrbitControls;
  material: THREE.MeshStandardMaterial;
  currentObject: THREE.Object3D | null;
  colorMaterials: THREE.MeshStandardMaterial[];
  floor: ReturnType<typeof createViewerFloor>;
}

function fitViewer(ctx: ViewerContext) {
  const object = ctx.currentObject;
  if (!object) return;
  frameObject(object, ctx.camera, ctx.controls);
  const box = visibleBounds(object);
  if (box.isEmpty()) { ctx.floor.visible = false; return; }
  const size = box.getSize(new THREE.Vector3());
  const floorSize = Math.max(size.x, size.y, size.z, 1) * 1.6;
  ctx.floor.scale.set(floorSize, floorSize, 1);
  ctx.floor.position.set(0, box.min.y - floorSize * 0.0001, 0);
  ctx.floor.visible = true;
}

export function ModelViewer({ surfaceClassName = 'h-full', fileId, needsSnapshot, onSnapshotCaptured, onError, showRotationControls, model, onOpenInSlicer, onRemoveFromCatalog }: Props) {
  const t = useT();
  const containerRef = useRef<HTMLDivElement>(null);
  const ctxRef = useRef<ViewerContext | null>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');
  const [compact, setCompact] = useState(false);
  const [legendOpen, setLegendOpen] = useState(false);
  const [autoRotating, setAutoRotating] = useState(false);
  const [fileColors, setFileColors] = useState(true);
  const [palette, setPalette] = useState<GeometryColor[]>([]);
  const [plates, setPlates] = useState<GeometryPlate[]>([]);
  const [selectedPlate, setSelectedPlate] = useState<number | null>(null);
  const [legend, setLegend] = useState<{ colorIndex: number; objectName?: string; plate?: number | null }[]>([]);
  const [error, setError] = useState<AppError | null>(null);
  const [noWebGL, setNoWebGL] = useState(false);

  // Set up renderer, scene, camera and light only once: a new
  // WebGL context per model change slowed the app down noticeably.
  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let renderer: THREE.WebGLRenderer;
    try {
      if (webglUnavailable) throw new Error('WebGL unavailable');
      renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true, preserveDrawingBuffer: true });
    } catch (err) {
      if (!webglUnavailable) console.error('[ModelViewer] WebGL not available:', err);
      webglUnavailable = true;
      setStatus('error');
      setNoWebGL(true);
      onError?.();
      return;
    }

    const scene = new THREE.Scene();
    const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 1000);
    renderer.setPixelRatio(window.devicePixelRatio);
    renderer.toneMapping = THREE.ACESFilmicToneMapping;
    container.appendChild(renderer.domElement);

    const controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.08;

    scene.add(new THREE.HemisphereLight(0xffffff, 0x706458, 1.0));
    const key = new THREE.DirectionalLight(0xffffff, 1.1);
    key.position.set(2, 3, 0.5);
    scene.add(key);
    const fill = new THREE.DirectionalLight(0xffffff, 0.15);
    fill.position.set(-1, -0.4, -1);
    scene.add(fill);

    // Coral close to the app accent, stands out from the background in both themes.
    const material = new THREE.MeshStandardMaterial({
      color: 0xd0603f,
      roughness: 0.55,
      metalness: 0.05,
      flatShading: true,
    });

    const floor = createViewerFloor(container);
    scene.add(floor);
    const themeObserver = new MutationObserver(() => {
      floor.material.uniforms.gridColor.value.copy(gridColor(container));
    });
    const appRoot = container.closest('[data-app]');
    if (appRoot) themeObserver.observe(appRoot, { attributes: true, attributeFilter: ['data-app'] });

    const resize = () => {
      const { clientWidth, clientHeight } = container;
      if (!clientWidth || !clientHeight) return;
      setCompact(clientHeight < 420 || clientWidth < 520);
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

    ctxRef.current = { scene, camera, renderer, controls, material, currentObject: null, colorMaterials: [], floor };

    return () => {
      cancelAnimationFrame(frameHandle);
      resizeObserver.disconnect();
      themeObserver.disconnect();
      floor.geometry.dispose();
      floor.material.dispose();
      ctxRef.current?.colorMaterials.forEach(m => m.dispose());
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

  // Model change: loads the new geometry and replaces only the object in
  // the existing scene instead of rebuilding the whole viewer.
  useEffect(() => {
    const ctx = ctxRef.current;
    if (!ctx) return;

    let cancelled = false;
    setStatus('loading');
    setError(null);
    setFileColors(true);
    setPalette([]);
    setLegend([]);
    setPlates([]);
    setSelectedPlate(null);
    setLegendOpen(false);
    ctx.floor.visible = false;
    if (ctx.currentObject) {
      ctx.scene.remove(ctx.currentObject);
      disposeObject(ctx.currentObject);
      ctx.currentObject = null;
    }
    ctx.colorMaterials.forEach(m => m.dispose());
    ctx.colorMaterials = [];

    invoke<ArrayBuffer>('get_model_geometry', { fileId })
      .then((buffer) => {
        if (cancelled) return;
        const { meshes, palette, plates } = decodeModelGeometry(buffer);
        setPlates(plates);
        ctx.colorMaterials = palette.map(color => new THREE.MeshStandardMaterial({
          color: color.color, roughness: 0.55, metalness: 0.05, flatShading: true,
        }));
        const object = buildGroup(meshes, ctx.material, ctx.colorMaterials);
        setPalette(palette);
        const entries: { colorIndex: number; objectName?: string; plate?: number | null }[] = [];
        for (const mesh of meshes) for (const range of mesh.groups ?? []) {
          if (range.colorIndex !== null && !entries.some(e => e.colorIndex === range.colorIndex && e.objectName === mesh.objectName && e.plate === mesh.plate)) {
            entries.push({ colorIndex: range.colorIndex, objectName: mesh.objectName, plate: mesh.plate });
          }
        }
        setLegend(entries);
        // 3MF/STL are Z-up, OrbitControls rotate around the world Y axis. The fixed
        // -90 degree rotation on X turns every azimuth rotation into a turntable
        // around the model's upright axis.
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
        fitViewer(ctx);
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
                console.error('[ModelViewer] snapshot failed:', err);
                onError?.();
              }
            });
          });
        }
      })
      .catch((err) => {
        console.error('[ModelViewer] loading failed:', err);
        if (!cancelled) {
          setStatus('error');
          setError(toAppError(err));
          onError?.();
        }
      });

    return () => {
      cancelled = true;
    };
  }, [fileId]);

  useEffect(() => {
    const ctx = ctxRef.current;
    ctx?.currentObject?.traverse(child => {
      if (child instanceof THREE.Mesh) child.material = fileColors ? child.userData.fileMaterials : ctx.material;
    });
  }, [fileColors]);

  const fit = () => {
    const ctx = ctxRef.current;
    if (ctx?.currentObject) fitViewer(ctx);
  };

  useEffect(() => {
    const ctx = ctxRef.current;
    if (!ctx?.currentObject || status !== 'ready') return;
    ctx.currentObject.traverse(child => {
      if (child instanceof THREE.Mesh) child.visible = selectedPlate === null || child.userData.plate === selectedPlate;
    });
    fitViewer(ctx);
  }, [selectedPlate, status]);

  const visibleLegend = legend.filter(entry => selectedPlate === null || entry.plate === selectedPlate)
    .filter((entry, index, entries) => entries.findIndex(e => e.colorIndex === entry.colorIndex && e.objectName === entry.objectName) === index);

  // OrbitControls pauses autoRotate itself while dragging and resumes afterwards.
  useEffect(() => {
    if (ctxRef.current) {
      ctxRef.current.controls.autoRotate = autoRotating;
    }
  }, [autoRotating]);

  // Rotates the camera by 15 degrees around the vertical axis. Via THREE.Spherical
  // because rotateLeft/rotateRight are private in OrbitControls.
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
    <div className={`w-full flex flex-col ${surfaceClassName === 'h-full' ? 'h-full' : ''}`}>
    <div data-viewer-surface data-compact={compact} className={`relative w-full shrink-0 ${surfaceClassName}`}>
      <div ref={containerRef} className="absolute inset-0" />
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center pointer-events-none">
          <div role="status" className="flex flex-col items-center gap-2 text-body text-[var(--ink-2)]">
            <span className="viewer-spinner" aria-hidden="true" />{t('viewerLoading')}
          </div>
        </div>
      )}
      {status === 'error' && <ViewerErrorCard key={fileId} error={error} noWebGL={noWebGL} compact={!showRotationControls}
        model={model} onOpenInSlicer={onOpenInSlicer} onRemoveFromCatalog={onRemoveFromCatalog} />}
      {showRotationControls && status === 'ready' && <>
        <button className={`viewer-pill absolute top-3 left-3 ${compact ? 'viewer-icon' : ''}`} aria-label={t('viewerReset')} title={t('viewerReset')}
          onClick={() => { setAutoRotating(false); fit(); }}>{compact ? <Icon name="reset-view" size={16} /> : t('viewerReset')}</button>
        {palette.length > 0 && <>
          <div role="group" aria-label={t('viewerColorsLegend')} className={`viewer-toggle absolute right-3 top-14 ${compact ? 'viewer-toggle-compact' : ''}`}>
            <button aria-pressed={fileColors} onClick={() => setFileColors(true)}>{t('viewerFileColors')}</button>
            <button aria-pressed={!fileColors} onClick={() => setFileColors(false)}>{t('viewerSingleColor')}</button>
          </div>
          {fileColors && compact && <button className="viewer-pill viewer-icon absolute bottom-12 left-3" aria-label={t('viewerLegend')} title={t('viewerLegend')}
            aria-expanded={legendOpen} onClick={() => setLegendOpen(open => !open)}><Icon name="info" size={14} /></button>}
          {fileColors && (!compact || legendOpen) && <div className={`viewer-legend absolute left-3 ${compact ? 'bottom-20' : 'bottom-14'}`}>
            <div className="ui-label text-[var(--ink-3)]">{t('viewerColorsLegend')}</div>
            {visibleLegend.map(entry => <div className="flex items-center gap-2" key={`${entry.colorIndex}:${entry.objectName}`}>
              <span className="w-3.5 h-3.5 rounded-full border border-[var(--line-strong)] shrink-0" style={{ background: palette[entry.colorIndex].color }} />
              <span>{palette[entry.colorIndex].name}</span><small className="text-caption text-[var(--ink-3)]">{entry.objectName}</small>
            </div>)}
          </div>}
        </>}
      </>}
      {showRotationControls && status === 'ready' && (
        <div className={`absolute bottom-3 left-1/2 -translate-x-1/2 flex items-center gap-1.5 bg-[var(--panel-2)] border border-[var(--line)] rounded-full p-1 shadow-[var(--shadow)] ${compact ? 'viewer-controls-compact' : ''}`}>
          <button className="viewer-pill" onClick={fit}>{t('viewerFit')}</button>
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
    {status === 'ready' && <PlateSelector plates={plates} selected={selectedPlate} onSelect={setSelectedPlate} />}
    </div>
  );
}
