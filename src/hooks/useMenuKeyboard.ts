import { useEffect, useRef, type RefObject, type KeyboardEvent as ReactKeyboardEvent } from 'react';

export function useMenuKeyboard(open: boolean, setOpen: (open: boolean) => void, container: RefObject<HTMLElement | null>) {
  const keyboardOpen = useRef(false);
  useEffect(() => {
    if (!open || !keyboardOpen.current) return;
    keyboardOpen.current = false;
    const menu = container.current?.querySelector('[role="menu"]');
    (menu?.querySelector<HTMLElement>('[aria-checked="true"]') ?? menu?.querySelector<HTMLElement>('[role^="menuitem"]'))?.focus();
  }, [open, container]);
  return {
    onTriggerKeyDown: (event: ReactKeyboardEvent) => {
      if (['ArrowDown', 'ArrowUp', 'Enter', ' '].includes(event.key)) {
        keyboardOpen.current = true;
        if (event.key.startsWith('Arrow')) { event.preventDefault(); setOpen(true); }
      }
    },
    onMenuKeyDown: (event: ReactKeyboardEvent) => {
      if (event.key === 'Tab') {
        // Keep the trigger as the native Tab starting point when the menu unmounts.
        container.current?.querySelector<HTMLButtonElement>('button')?.focus();
        setOpen(false);
        return;
      }
      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      const items = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role^="menuitem"]')).filter(el => !el.matches(':disabled'));
      if (!items.length) return;
      event.preventDefault();
      const index = items.indexOf(document.activeElement as HTMLElement);
      const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (index + (event.key === 'ArrowDown' ? 1 : -1) + items.length) % items.length;
      items[next].focus();
    },
  };
}
