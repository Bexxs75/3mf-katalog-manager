import { useLayoutEffect, useRef, type RefObject } from 'react';

const focusableSelector = 'button, [href], input, select, textarea, [tabindex], [contenteditable="true"]';
const modals: HTMLElement[] = [];
// Portals belong to their anchor's dialog even though they live under body.
const popups = new Map<HTMLElement, HTMLElement>();
export function registerDialogPopup(popup: HTMLElement, anchor: HTMLElement) {
  popups.set(popup, anchor);
  return () => { popups.delete(popup); };
}
function belongsTo(dialog: HTMLElement, node: Node): boolean {
  if (dialog.contains(node)) return true;
  return [...popups].some(([popup, anchor]) => popup.contains(node) && belongsTo(dialog, anchor));
}
function focusable(root: HTMLElement) {
  return Array.from(root.querySelectorAll<HTMLElement>(focusableSelector)).filter(el =>
    el.tabIndex >= 0 && !el.matches(':disabled') && !el.closest('[hidden], [inert], [aria-hidden="true"]') &&
    getComputedStyle(el).display !== 'none' && getComputedStyle(el).visibility !== 'hidden');
}

interface Options {
  open: boolean;
  onClose: () => void;
  busy?: boolean;
  returnFocus?: RefObject<HTMLElement | null>;
  initialFocus?: string | RefObject<HTMLElement | null>;
}

/** Shared focus and keyboard boundary; callers supply dialog semantics and name. */
export function useModalDialog<T extends HTMLElement = HTMLDivElement>({ open, ...options }: Options) {
  const ref = useRef<T>(null);
  const latest = useRef(options);
  latest.current = options;
  const wasOpen = useRef(false);
  const previous = useRef<HTMLElement | null>(null);
  // Capture before React commits descendants with autoFocus.
  if (open && !wasOpen.current) previous.current = document.activeElement as HTMLElement | null;
  wasOpen.current = open;

  useLayoutEffect(() => {
    const dialog = ref.current;
    if (!open || !dialog) return;
    const opener = latest.current.returnFocus?.current ?? previous.current;
    modals.push(dialog);
    const top = () => modals[modals.length - 1] === dialog;
    const focusFirst = () => {
      const initial = latest.current.initialFocus;
      const target = typeof initial === 'string' ? dialog.querySelector<HTMLElement>(initial) : initial?.current;
      (target && !target.matches(':disabled') ? target : focusable(dialog)[0] ?? dialog).focus({ preventScroll: true });
    };
    focusFirst();
    const focusin = (event: FocusEvent) => {
      if (top() && event.target instanceof Node && !belongsTo(dialog, event.target)) focusFirst();
    };
    const keydown = (event: KeyboardEvent) => {
      if (!top() || event.defaultPrevented) return;
      if (event.key === 'Escape') {
        event.preventDefault(); event.stopImmediatePropagation();
        if (!latest.current.busy) latest.current.onClose();
      } else if (event.key === 'Tab') {
        const elements = focusable(dialog);
        for (const [popup, anchor] of popups) {
          if (belongsTo(dialog, anchor)) elements.push(...focusable(popup));
        }
        const first = elements[0]; const last = elements[elements.length - 1];
        const active = document.activeElement;
        if (!first) { event.preventDefault(); dialog.focus(); }
        else if (event.shiftKey && (active === first || !elements.includes(active as HTMLElement))) {
          event.preventDefault(); last.focus();
        } else if (!event.shiftKey && (active === last || !elements.includes(active as HTMLElement))) {
          event.preventDefault(); first.focus();
        }
      }
    };
    const pointer = (event: Event) => {
      if (!top() || !(event.target instanceof Node) || belongsTo(dialog, event.target)) return;
      // Backdrops may dismiss the dialog; clicks on background controls may not.
      if (event.target instanceof HTMLElement && event.target.hasAttribute('data-modal-backdrop') && event.target.nextElementSibling === dialog) return;
      if (event.target === dialog.parentElement && dialog.parentElement !== document.body) return;
      event.preventDefault(); event.stopImmediatePropagation();
    };
    document.addEventListener('focusin', focusin);
    // Bubble so a nested picker can consume Escape before its parent dialog.
    document.addEventListener('keydown', keydown);
    for (const type of ['pointerdown', 'mousedown', 'click']) document.addEventListener(type, pointer, true);
    return () => {
      modals.splice(modals.indexOf(dialog), 1);
      document.removeEventListener('focusin', focusin);
      document.removeEventListener('keydown', keydown);
      for (const type of ['pointerdown', 'mousedown', 'click']) document.removeEventListener(type, pointer, true);
      const target = latest.current.returnFocus?.current ?? opener;
      if (target?.isConnected) target.focus({ preventScroll: true });
    };
  }, [open]);
  return ref;
}
