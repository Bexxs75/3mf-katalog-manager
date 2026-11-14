import { useEffect, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import type { FilamentSpool } from '../types';
import { loadSpoolImage, spoolImageRevision, subscribeSpoolImages } from '../lib/spoolImages';
import { reportMetadataError } from '../lib/metadataErrors';

/** Keep list DTOs image-free. IntersectionObserver also respects the scroll container. */
export function SpoolImage({ spool, className, fallback }: { spool: FilamentSpool; className: string; fallback: ReactNode }) {
  const ref = useRef<HTMLSpanElement>(null);
  const [image, setImage] = useState<string | null>(null);
  const revision = useSyncExternalStore(subscribeSpoolImages, spoolImageRevision);
  useEffect(() => {
    setImage(null);
    if (import.meta.env.VITE_TARGET !== 'web' || !spool.hasImage || !ref.current) return;
    let mounted = true;
    let visible = false;
    let requested = false;
    const observer = new IntersectionObserver(entries => {
      visible = entries.some(e => e.isIntersecting);
      if (!visible) { setImage(null); requested = false; return; }
      if (requested) return;
      requested = true;
      void loadSpoolImage(spool.id, () => mounted && visible).then(value => {
        if (!mounted) return;
        if (value === undefined) requested = false;
        else if (visible) setImage(value);
      }).catch(error => { if (mounted) reportMetadataError(error); });
    });
    observer.observe(ref.current);
    return () => { mounted = false; observer.disconnect(); };
  }, [spool.id, spool.hasImage, revision]);
  const value = spool.imagePng || image;
  return <span ref={ref} className="inline-flex shrink-0">{value ? <img draggable={false} alt="" src={`data:image/png;base64,${value}`} className={className} /> : fallback}</span>;
}
