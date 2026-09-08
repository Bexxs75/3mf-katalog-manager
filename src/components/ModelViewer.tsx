import { useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import * as THREE from 'three';
import { OrbitControls } from 'three/examples/jsm/controls/OrbitControls.js';
import { STLLoader } from 'three/examples/jsm/loaders/STLLoader.js';
import { ThreeMFLoader } from 'three/examples/jsm/loaders/3MFLoader.js';

interface GeometryPayload {
  extension: string;
  dataBase64: string;
}

interface Props {
  fileId: string;
}

function base64ToArrayBuffer(base64: string): ArrayBuffer {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes.buffer;
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

export function ModelViewer({ fileId }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [status, setStatus] = useState<'loading' | 'ready' | 'error'>('loading');

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let disposed = false;
    let frameHandle = 0;

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

    const animate = () => {
      frameHandle = requestAnimationFrame(animate);
      controls.update();
      renderer.render(scene, camera);
    };
    animate();

    setStatus('loading');
    invoke<GeometryPayload>('get_model_geometry', { fileId })
      .then((payload) => {
        if (disposed) return;
        const buffer = base64ToArrayBuffer(payload.dataBase64);

        let object: THREE.Object3D;
        if (payload.extension === 'stl') {
          const geometry = new STLLoader().parse(buffer);
          geometry.computeVertexNormals();
          object = new THREE.Mesh(geometry, material);
        } else if (payload.extension === '3mf') {
          const group = new ThreeMFLoader().parse(buffer);
          group.traverse((child) => {
            if (child instanceof THREE.Mesh) {
              child.material = material;
            }
          });
          object = group;
        } else {
          throw new Error(`nicht unterstütztes Format: ${payload.extension}`);
        }

        scene.add(object);
        resize();
        frameObject(object, camera, controls);
        setStatus('ready');
      })
      .catch((err) => {
        console.error('[ModelViewer] Laden fehlgeschlagen:', err);
        if (!disposed) setStatus('error');
      });

    return () => {
      disposed = true;
      cancelAnimationFrame(frameHandle);
      resizeObserver.disconnect();
      controls.dispose();
      scene.traverse((child) => {
        if (child instanceof THREE.Mesh) {
          child.geometry.dispose();
        }
      });
      material.dispose();
      renderer.dispose();
      container.removeChild(renderer.domElement);
    };
  }, [fileId]);

  return (
    <div className="relative w-full h-full">
      <div ref={containerRef} className="absolute inset-0" />
      {status === 'loading' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none">
          Lädt Vorschau …
        </div>
      )}
      {status === 'error' && (
        <div className="absolute inset-0 grid place-items-center font-mono-ui text-[11px] text-[var(--ink-3)] pointer-events-none px-4 text-center">
          Vorschau nicht verfügbar
        </div>
      )}
    </div>
  );
}
