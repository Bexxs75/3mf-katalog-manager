import { useCallback, useLayoutEffect, useRef } from 'react';

export function useCatalogScroll(detailOpen: boolean, viewKey: string, selectedId: string | null) {
  const container = useRef<HTMLDivElement | null>(null);
  const saved = useRef<number | null>(null);
  const last = useRef<number>(0);
  const previousKey = useRef(viewKey);
  const trackScroll = useCallback(() => {
    if (container.current) last.current = container.current.scrollTop;
  }, []);
  const attach = useCallback((node: HTMLDivElement | null) => {
    if (container.current) {
      container.current.removeEventListener('scroll', trackScroll);
      // Chromium/WebView2 has already clamped scrollTop to the remaining
      // padding when the ref is detached, so the value tracked on scroll
      // events is the only reliable one.
      if (!node) saved.current = last.current;
    }
    container.current = node;
    if (node) node.addEventListener('scroll', trackScroll, { passive: true });
  }, [trackScroll]);

  useLayoutEffect(() => {
    if (previousKey.current !== viewKey) {
      saved.current = null;
      last.current = 0;
    }
    previousKey.current = viewKey;
    const node = container.current;
    if (detailOpen || !node || saved.current === null) return;
    node.scrollTop = saved.current;
    last.current = node.scrollTop;
    saved.current = null;
    const selected = Array.from(node.querySelectorAll<HTMLElement>('[data-model-id]'))
      .find((element) => element.dataset.modelId === selectedId);
    if (!selected) return;
    const viewport = node.getBoundingClientRect();
    const bounds = selected.getBoundingClientRect();
    if (bounds.top < viewport.top || bounds.bottom > viewport.bottom) {
      selected.scrollIntoView({ block: 'nearest' });
    }
  }, [detailOpen, viewKey, selectedId]);
  return attach;
}
