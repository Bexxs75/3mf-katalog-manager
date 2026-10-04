import { useLayoutEffect, useState, type RefObject } from 'react';

interface Options {
  containerRef: RefObject<HTMLElement | null>;
  itemCount: number;
  columns: number;
  rowHeight: number;
  overscan?: number;
  offsetTop?: number;
}
export function useWindowedRows({ containerRef, itemCount, columns, rowHeight, overscan = 2, offsetTop = 0 }: Options) {
  const [viewport, setViewport] = useState({ top: 0, height: 600 });
  useLayoutEffect(() => {
    let frame: number | null = null;
    let dispose = () => {};
    const attach = () => {
      const container = containerRef.current;
      if (!container) return;
      const read = () => {
        setViewport(previous => {
          const next = { top: container.scrollTop, height: container.clientHeight };
          return previous.top === next.top && previous.height === next.height ? previous : next;
        });
      };
      const scroll = () => {
        if (frame !== null) return;
        frame = requestAnimationFrame(() => { frame = null; read(); });
      };
      read();
      container.addEventListener('scroll', scroll, { passive: true });
      const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(read);
      observer?.observe(container);
      dispose = () => { container.removeEventListener('scroll', scroll); observer?.disconnect(); };
    };
    // Parent refs attach after child layout effects during the same commit.
    if (containerRef.current) attach();
    else frame = requestAnimationFrame(() => { frame = null; attach(); });
    return () => { dispose(); if (frame !== null) cancelAnimationFrame(frame); };
  }, [containerRef]);
  const cols = Math.max(1, Math.floor(columns));
  const height = Math.max(1, rowHeight);
  const rows = Math.ceil(itemCount / cols);
  const start = Math.min(rows, Math.max(0, Math.floor((viewport.top - offsetTop) / height) - overscan));
  const end = Math.max(start, Math.min(rows, Math.max(0, Math.ceil((viewport.top + viewport.height - offsetTop) / height) + overscan)));
  return {
    startIndex: Math.min(itemCount, start * cols),
    endIndex: Math.min(itemCount, end * cols),
    topSpacer: start * height,
    bottomSpacer: (rows - end) * height,
  };
}
