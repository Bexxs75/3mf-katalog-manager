import { useCallback, useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import type { ThingSummary } from '../types';

export const thingButton = 'h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)]';
/** Copies a link and tells how that went. Shared by the cards and the detail page. */
export function useCopyLink() {
  const [result, setResult] = useState<'copied' | 'failed' | null>(null);
  const copy = useCallback(async (url: string) => {
    try {
      await navigator.clipboard.writeText(url);
      setResult('copied');
    } catch {
      setResult('failed');
    }
  }, []);
  return { result, copy };
}

/** True once the element has been on screen. Without IntersectionObserver (old webviews, tests) it is true at once. */
function useSeen(ref: React.RefObject<HTMLElement | null>) {
  const [seen, setSeen] = useState(typeof IntersectionObserver === 'undefined');
  useEffect(() => {
    const node = ref.current;
    if (seen || !node) return;
    const observer = new IntersectionObserver(entries => {
      if (entries.some(e => e.isIntersecting)) { setSeen(true); observer.disconnect(); }
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [seen, ref]);
  return seen;
}

/** Preview of one hit. The image is fetched only when the card comes into view. */
export function ThingImage({ thing, image, onNeedImage, large }: {
  thing: ThingSummary;
  /** `undefined` = not asked yet, `null` = none available. */
  image: string | null | undefined;
  onNeedImage: (id: number) => void;
  large?: boolean;
}) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const seen = useSeen(ref);
  useEffect(() => {
    if (seen && thing.previewAvailable && image === undefined) onNeedImage(thing.id);
  }, [seen, thing.id, thing.previewAvailable, image, onNeedImage]);
  const box = `w-full ${large ? 'aspect-[4/3]' : 'aspect-[4/3]'} rounded bg-[var(--plate)] overflow-hidden grid place-items-center text-caption text-[var(--ink-3)]`;
  if (image) return <div ref={ref} className={box}><img src={image} alt={thing.name} className="w-full h-full object-cover" /></div>;
  const waiting = thing.previewAvailable && image === undefined;
  return <div ref={ref} className={box} role={waiting ? 'img' : undefined} aria-label={waiting ? t('tvImageLoading') : undefined}>{waiting ? '' : t('tvNoImage')}</div>;
}

