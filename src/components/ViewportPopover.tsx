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
    const place = () => {
      // Measure at the natural position, then apply the offset before paint.
      // An autofocused child may already have scrolled the page sideways to
      // reveal itself, so the position is taken in document coordinates.
      el.style.transform = '';
      const rect = el.getBoundingClientRect();
      const dx = popoverShift(rect.left + window.scrollX, rect.width, document.documentElement.clientWidth, VIEWPORT_MARGIN);
      el.style.transform = dx === 0 ? '' : `translateX(${dx}px)`;
    };
    place();
    window.addEventListener('resize', place);
    return () => window.removeEventListener('resize', place);
  }, []);

  return <div ref={ref} style={style} {...props} />;
}
