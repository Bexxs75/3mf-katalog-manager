import { useCallback, useLayoutEffect, useRef } from 'react';

export function useCatalogScroll(detailOpen: boolean, viewKey: string, selectedId: string | null) {
  const container = useRef<HTMLDivElement | null>(null);
  const saved = useRef<number | null>(null);
  const previousKey = useRef(viewKey);
  const attach = useCallback((node: HTMLDivElement | null) => {
    if (!node && container.current) saved.current = container.current.scrollTop;
    container.current = node;
  }, []);

  useLayoutEffect(() => {
    if (previousKey.current !== viewKey) saved.current = null;
    previousKey.current = viewKey;
    const node = container.current;
    if (detailOpen || !node || saved.current === null) return;
    node.scrollTop = saved.current;
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
