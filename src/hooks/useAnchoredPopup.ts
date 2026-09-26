import { useEffect, useRef, useState, type RefObject } from 'react';

// Fixed-positioned popup via portal in document.body so overflowing dialog
// scroll containers don't clip it (SpoolPicker, ModelPicker).

const VIEWPORT_MARGIN = 8;

export interface AnchoredPopupStyle {
  position: 'fixed';
  top: number;
  left: number;
  width: number;
}

function popupPosition(rect: DOMRect, minWidth: number): AnchoredPopupStyle {
  const width = Math.min(Math.max(rect.width, minWidth), window.innerWidth - VIEWPORT_MARGIN * 2);
  let left = rect.left;
  if (left + width > window.innerWidth - VIEWPORT_MARGIN) {
    // Doesn't fit into the viewport on the right - align to the anchor's right
    // edge (and still keep it inside the viewport).
    left = Math.max(VIEWPORT_MARGIN, rect.right - width);
  }
  return { position: 'fixed', top: rect.bottom + 4, left, width };
}

/**
 * Positions a popup fixed at the anchor and closes it on outside click,
 * resize and real scrolling. Scroll events from the popup itself (e.g. its
 * scrollIntoView) don't close it, otherwise it would close right away.
 * Flips upward when there is too little room below (measured after the
 * first render).
 */
export function useAnchoredPopup<Anchor extends HTMLElement, Popup extends HTMLElement>(
  anchorRef: RefObject<Anchor | null>,
  open: boolean,
  onClose: () => void,
  minWidth: number,
): { popupRef: RefObject<Popup | null>; style: AnchoredPopupStyle | null } {
  const popupRef = useRef<Popup>(null);
  const [style, setStyle] = useState<AnchoredPopupStyle | null>(null);

  // Keep the latest onClose reference so the listeners aren't re-registered on every render.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  // Compute the position on open and keep it on close so the popup
  // doesn't briefly appear at 0/0 on the next open.
  useEffect(() => {
    if (!open) return;
    const anchor = anchorRef.current;
    if (!anchor) return;
    setStyle(popupPosition(anchor.getBoundingClientRect(), minWidth));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, minWidth]);

  // After the first render with the position above, check the popup's
  // actual height and flip upward if there isn't enough room below
  // but more above.
  useEffect(() => {
    if (!open || !style) return;
    const anchor = anchorRef.current;
    const popup = popupRef.current;
    if (!anchor || !popup) return;
    const anchorRect = anchor.getBoundingClientRect();
    const popupHeight = popup.getBoundingClientRect().height;
    const spaceBelow = window.innerHeight - anchorRect.bottom - VIEWPORT_MARGIN;
    const spaceAbove = anchorRect.top - VIEWPORT_MARGIN;
    if (popupHeight > spaceBelow && spaceAbove > spaceBelow) {
      const flippedTop = anchorRect.top - 4 - popupHeight;
      setStyle((s) => (s && Math.abs(s.top - flippedTop) > 0.5 ? { ...s, top: flippedTop } : s));
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, style]);

  useEffect(() => {
    if (!open) return;
    const closeOnResize = () => onCloseRef.current();
    const closeOnScroll = (e: Event) => {
      // For a scroll on `window`/`document` itself, `target` is not a
      // Node (no `.contains()`) - then by definition it isn't a scroll
      // inside the popup.
      const target = e.target;
      if (target instanceof Node && popupRef.current?.contains(target)) return;
      onCloseRef.current();
    };
    window.addEventListener('scroll', closeOnScroll, true);
    window.addEventListener('resize', closeOnResize);
    return () => {
      window.removeEventListener('scroll', closeOnScroll, true);
      window.removeEventListener('resize', closeOnResize);
    };
  }, [open]);

  // A click outside anchor and popup closes it.
  useEffect(() => {
    if (!open) return;
    const onDocMouseDown = (e: MouseEvent) => {
      const target = e.target as Node;
      if (anchorRef.current?.contains(target)) return;
      if (popupRef.current?.contains(target)) return;
      onCloseRef.current();
    };
    document.addEventListener('mousedown', onDocMouseDown);
    return () => document.removeEventListener('mousedown', onDocMouseDown);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  return { popupRef, style };
}
