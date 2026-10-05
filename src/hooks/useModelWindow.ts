import { useContext, useLayoutEffect, useMemo, useRef, useState, type RefObject } from 'react';
import { ModelLayoutContext } from './ModelLayoutContext';
import { useWindowedRows } from './useWindowedRows';

interface Options {
  ids: string[];
  containerRef?: RefObject<HTMLDivElement | null>;
  minWidth?: number;
  gap: number;
  fallbackHeight: number;
  disabled?: boolean;
}
/** Measures the local section, while scrolling always belongs to the workspace. */
export function useModelWindow({ ids, containerRef, minWidth, gap, fallbackHeight, disabled }: Options) {
  const rootRef = useRef<HTMLDivElement>(null);
  const discoveredContainer = useRef<HTMLDivElement | null>(null);
  const scroller = containerRef ?? discoveredContainer;
  const [geometry, setGeometry] = useState({ columns: minWidth ? 3 : 1, rowHeight: fallbackHeight + gap, offsetTop: 0 });
  const [measured, setMeasured] = useState(false);
  const registry = useContext(ModelLayoutContext);
  const token = useRef({});
  useLayoutEffect(() => {
    if (!containerRef) discoveredContainer.current = rootRef.current?.closest<HTMLDivElement>('[data-catalog-scroller]') ?? null;
  });
  const windowed = useWindowedRows({ containerRef: scroller, itemCount: ids.length, ...geometry });
  const virtual = !disabled && (!!containerRef || !!scroller.current);
  const startIndex = virtual ? windowed.startIndex : 0;
  const endIndex = virtual ? windowed.endIndex : ids.length;
  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) return;
    const container = scroller.current;
    const card = root.querySelector<HTMLElement>('[data-model-id]');
    const measure = () => {
      const rect = root.getBoundingClientRect();
      const width = rect.width;
      // A hidden or not-yet-laid-out section must not collapse to one giant card.
      if (!width) return;
      const columns = minWidth ? Math.max(1, Math.floor((width + gap) / (minWidth + gap))) : 1;
      setMeasured(true);
      const offsetTop = container ? rect.top - container.getBoundingClientRect().top - container.clientTop + container.scrollTop : 0;
      const measuredHeight = card?.getBoundingClientRect().height ?? 0;
      setGeometry(previous => {
        const rowHeight = measuredHeight > 0 ? measuredHeight + gap : previous.rowHeight;
        return previous.columns === columns && previous.rowHeight === rowHeight && previous.offsetTop === offsetTop
        ? previous : { columns, rowHeight, offsetTop };
      });
    };
    measure();
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(measure);
    observer?.observe(root);
    if (root.parentElement) observer?.observe(root.parentElement);
    if (card) observer?.observe(card);
    const frame = requestAnimationFrame(measure);
    return () => { observer?.disconnect(); cancelAnimationFrame(frame); };
  }, [scroller, minWidth, gap, fallbackHeight, startIndex, endIndex, ids]);
  const layout = useMemo(() => ({ offsetTop: geometry.offsetTop, columns: geometry.columns, order: ids, scrollToIndex: (index: number) => {
    if (index < 0 || index >= ids.length) return;
    const container = scroller.current;
    if (!container) return;
    const top = geometry.offsetTop + Math.floor(index / geometry.columns) * geometry.rowHeight;
    const bottom = top + geometry.rowHeight - gap;
    if (top < container.scrollTop) container.scrollTop = top;
    else if (bottom > container.scrollTop + container.clientHeight) container.scrollTop = bottom - container.clientHeight;
    container.dispatchEvent(new Event('scroll'));
  } }), [geometry, ids, scroller, gap]);
  useLayoutEffect(() => {
    if (!registry) return;
    const key = token.current;
    registry.layouts.set(key, layout);
    return () => { registry.layouts.delete(key); };
  }, [registry, layout]);
  return { rootRef, measured, imageStartIndex: containerRef ? windowed.startIndex : startIndex,
    imageEndIndex: containerRef ? windowed.endIndex : endIndex, columns: geometry.columns, startIndex, endIndex,
    topSpacer: virtual ? windowed.topSpacer : 0,
    bottomSpacer: virtual ? windowed.bottomSpacer : 0,
    // A completely offscreen section has no rendered row with its final gap.
    emptyHeight: virtual && startIndex === endIndex ? Math.max(0, Math.ceil(ids.length / geometry.columns) * geometry.rowHeight - gap) : null,
  };
}
