import { useLayoutEffect, useRef, type HTMLAttributes } from 'react';
import { popoverShift } from '../lib/popoverShift';

const VIEWPORT_MARGIN = 8;

/**
 * Absolutely positioned popover that is moved sideways until it lies inside
 * the window. Without it a popover opening below a button near the right edge
 * sticks out of the window, where its suggestion list and scrollbar cannot be
 * used and focus changes scroll the page sideways.
 */
export function ViewportPopover({ style, ...props }: HTMLAttributes<HTMLDivElement>) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let placed: { left: number; top: number; width: number; height: number };
    const place = () => {
      // Measure at the natural position, then apply the offset before paint.
      el.style.transform = '';
      const rect = el.getBoundingClientRect();
      const dx = popoverShift(rect.left, rect.width, document.documentElement.clientWidth, VIEWPORT_MARGIN);
      el.style.transform = dx === 0 ? '' : `translateX(${dx}px)`;
      // Read back the rendered position to avoid repeated writes from subpixel rounding.
      placed = el.getBoundingClientRect();
    };
    place();
    window.addEventListener('resize', place);
    // The direct parent anchors all toolbar popovers; its parent is the
    // wrapping toolbar, whose width changes when the sidebar is dragged.
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(place);
    observer?.observe(el);
    if (el.parentElement) {
      observer?.observe(el.parentElement);
      if (el.parentElement.parentElement) observer?.observe(el.parentElement.parentElement);
    }

    // ResizeObserver cannot see movement without a size change (e.g. wrapping
    // siblings). Read the visible box while open, but only write on a change.
    let frame: number;
    const trackPosition = () => {
      const rect = el.getBoundingClientRect();
      if (rect.left !== placed.left || rect.top !== placed.top ||
          rect.width !== placed.width || rect.height !== placed.height) place();
      frame = requestAnimationFrame(trackPosition);
    };
    frame = requestAnimationFrame(trackPosition);
    return () => {
      window.removeEventListener('resize', place);
      observer?.disconnect();
      cancelAnimationFrame(frame);
    };
  }, []);

  return <div ref={ref} style={style} {...props} />;
}
