import { useEffect, type RefObject } from 'react';

export function useDismissableMenu(
  open: boolean,
  onClose: (open: boolean) => void,
  menuRef: RefObject<HTMLElement | null>,
  triggerRef: RefObject<HTMLButtonElement | null>,
) {
  useEffect(() => {
    if (!open) return;
    const outside = (event: MouseEvent) => {
      if (event.target instanceof Node && !menuRef.current?.contains(event.target)) onClose(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      onClose(false);
      triggerRef.current?.focus();
    };
    document.addEventListener('mousedown', outside);
    document.addEventListener('click', outside);
    // Capture before the application's selection/detail Escape handlers.
    window.addEventListener('keydown', escape, true);
    return () => {
      document.removeEventListener('mousedown', outside);
      document.removeEventListener('click', outside);
      window.removeEventListener('keydown', escape, true);
    };
  }, [open, onClose, menuRef, triggerRef]);
}
