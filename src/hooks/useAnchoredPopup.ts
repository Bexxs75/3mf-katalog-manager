import { useEffect, useRef, useState, type RefObject } from 'react';

// Fixed positioniertes Popup per Portal in document.body, damit ueberlaufende
// Dialog-Scrollcontainer es nicht abschneiden (SpoolPicker, ModelPicker).

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
    // Passt rechts nicht mehr in den Viewport - am rechten Rand des Ankers
    // ausrichten (und noch innerhalb des Viewports halten).
    left = Math.max(VIEWPORT_MARGIN, rect.right - width);
  }
  return { position: 'fixed', top: rect.bottom + 4, left, width };
}

/**
 * Positioniert ein Popup fixed am Anker und schliesst es bei Klick ausserhalb,
 * Resize und echtem Scroll. Scroll-Events aus dem Popup selbst (z.B. sein
 * scrollIntoView) schliessen es nicht, sonst ginge es sofort wieder zu.
 * Klappt nach oben, wenn unten zu wenig Platz ist (nach dem ersten Rendern
 * gemessen).
 */
export function useAnchoredPopup<Anchor extends HTMLElement, Popup extends HTMLElement>(
  anchorRef: RefObject<Anchor | null>,
  open: boolean,
  onClose: () => void,
  minWidth: number,
): { popupRef: RefObject<Popup | null>; style: AnchoredPopupStyle | null } {
  const popupRef = useRef<Popup>(null);
  const [style, setStyle] = useState<AnchoredPopupStyle | null>(null);

  // Neueste onClose-Referenz halten, damit die Listener nicht bei jedem Render neu angemeldet werden.
  const onCloseRef = useRef(onClose);
  useEffect(() => {
    onCloseRef.current = onClose;
  });

  // Position beim Oeffnen berechnen und beim Schliessen behalten, damit das
  // Popup beim naechsten Oeffnen nicht kurz bei 0/0 erscheint.
  useEffect(() => {
    if (!open) return;
    const anchor = anchorRef.current;
    if (!anchor) return;
    setStyle(popupPosition(anchor.getBoundingClientRect(), minWidth));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, minWidth]);

  // Nach dem ersten Rendern mit der obigen Position die tatsaechliche Hoehe
  // des Popups pruefen und nach oben klappen, wenn unten nicht genug Platz
  // ist, aber oben mehr.
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
      // `target` ist bei einem Scroll auf `window`/`document` selbst kein
      // Node (kein `.contains()`) - dann ist es per Definition kein Scroll
      // innerhalb des Popups.
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

  // Klick ausserhalb von Anker und Popup schliesst es.
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
