import { useLayoutEffect, useRef, type HTMLAttributes } from 'react';
import { popoverFit } from '../lib/popoverFit';
import { popoverShift } from '../lib/popoverShift';

const VIEWPORT_MARGIN = 8;

/**
 * Absolutely positioned popover that is moved sideways until it lies inside
 * the window. Without it a popover opening below a button near the right edge
 * sticks out of the window, where its suggestion list and scrollbar cannot be
 * used and focus changes scroll the page sideways.
 */
export function ViewportPopover({ style, fitHeight = false, ...props }: HTMLAttributes<HTMLDivElement> & { fitHeight?: boolean }) {
  const ref = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    let placed: DOMRect;
    let anchorTop: number | undefined;
    let anchorHeight: number | undefined;
    let contentHeight: number;
    const anchor = el.parentElement;
    const place = () => {
      // Measure at the natural position, then apply the offset before paint.
      el.style.transform = '';
      if (fitHeight) el.style.maxHeight = '';
      const rect = el.getBoundingClientRect();
      const dx = popoverShift(rect.left, rect.width, document.documentElement.clientWidth, VIEWPORT_MARGIN);
      let dy = 0;
      if (fitHeight && anchor) {
        const anchorRect = anchor.getBoundingClientRect();
        const viewportHeight = document.documentElement.clientHeight;
        const rem = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
        // Match the CSS fallback cap, including enlarged root font sizes.
        const cap = Math.max(11 * rem, viewportHeight * 0.6);
        const fit = popoverFit(rect.top, anchorRect.top, rect.height, viewportHeight,
          VIEWPORT_MARGIN, 11 * rem, cap, Math.max(0, rect.top - anchorRect.bottom));
        el.style.maxHeight = `${fit.maxHeight}px`;
        // Measure after limiting height: upward placement depends on the actual box height.
        const fittedHeight = el.getBoundingClientRect().height;
        dy = fit.top - rect.top;
        if (fit.top < rect.top) dy += Math.min(rect.height, fit.maxHeight) - fittedHeight;
        anchorTop = anchorRect.top;
        anchorHeight = anchorRect.height;
      }
      el.style.transform = [dx === 0 ? '' : `translateX(${dx}px)`,
        dy === 0 ? '' : `translateY(${dy}px)`].filter(Boolean).join(' ');
      // Read back the rendered position to avoid repeated writes from subpixel rounding.
      placed = el.getBoundingClientRect();
      contentHeight = el.scrollHeight;
    };
    place();
    window.addEventListener('resize', place);
    // The direct parent anchors all toolbar popovers; its parent is the
    // wrapping toolbar, whose width changes when the sidebar is dragged.
    const observer = typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => fitHeight ? checkPosition() : place());
    observer?.observe(el);
    if (el.parentElement) {
      observer?.observe(el.parentElement);
      if (el.parentElement.parentElement) observer?.observe(el.parentElement.parentElement);
    }

    // ResizeObserver cannot see movement without a size change (e.g. wrapping
    // siblings). Read the visible box while open, but only write when relevant geometry changes.
    let frame: number;
    const checkPosition = () => {
      const rect = el.getBoundingClientRect();
      const anchorRect = fitHeight ? anchor?.getBoundingClientRect() : undefined;
      if (rect.left !== placed.left || rect.width !== placed.width || (fitHeight && (
        rect.top !== placed.top || rect.height !== placed.height || el.scrollHeight !== contentHeight ||
        anchorRect?.top !== anchorTop || anchorRect?.height !== anchorHeight))) place();
    };
    const trackPosition = () => {
      checkPosition();
      frame = requestAnimationFrame(trackPosition);
    };
    frame = requestAnimationFrame(trackPosition);
    return () => {
      window.removeEventListener('resize', place);
      observer?.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [fitHeight]);

  return <div ref={ref} style={style} {...props} />;
}
