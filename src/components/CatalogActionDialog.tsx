import { useEffect, useId, useRef, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';

/** Shared keyboard boundary for catalog-only confirmations. */
export function CatalogActionDialog({ title, children, onClose, returnFocus }: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  returnFocus?: RefObject<HTMLElement | null>;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const titleId = useId();
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const previous = returnFocus?.current ?? document.activeElement as HTMLElement | null;
    const dialog = ref.current!;
    (dialog.querySelector<HTMLElement>('[data-initial-focus]') ?? dialog).focus();
    const keydown = (event: KeyboardEvent) => {
      // Keep global catalog shortcuts from acting behind this modal.
      event.stopPropagation();
      if (event.key === 'Escape') {
        event.preventDefault(); event.stopPropagation(); close.current();
      }
      if (event.key === 'Tab') {
        const buttons = Array.from(dialog.querySelectorAll<HTMLElement>('button:not(:disabled), [href], input:not(:disabled), [tabindex="0"]'));
        const first = buttons[0]; const last = buttons[buttons.length - 1];
        if (!first) { event.preventDefault(); dialog.focus(); }
        else if (event.shiftKey && (document.activeElement === first || !dialog.contains(document.activeElement))) {
          event.preventDefault(); last.focus();
        } else if (!event.shiftKey && (document.activeElement === last || !dialog.contains(document.activeElement))) {
          event.preventDefault(); first.focus();
        }
      }
    };
    const focusin = (event: FocusEvent) => {
      if (!dialog.contains(event.target as Node)) {
        (dialog.querySelector<HTMLElement>('button:not(:disabled)') ?? dialog).focus();
      }
    };
    document.addEventListener('keydown', keydown, true);
    document.addEventListener('focusin', focusin);
    return () => {
      document.removeEventListener('keydown', keydown, true);
      document.removeEventListener('focusin', focusin);
      const target = returnFocus?.current ?? previous;
      if (target?.isConnected) target.focus();
    };
  }, [returnFocus]);
  return createPortal(
    <div className="fixed inset-0 z-[100] bg-black/45 flex items-center justify-center p-4" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div ref={ref} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className="w-full max-w-[540px] max-h-[90vh] overflow-auto rounded-lg border border-[var(--line)] bg-[var(--panel)] text-[var(--ink)] p-5 shadow-[var(--shadow)] text-[13px] space-y-4">
        <h2 id={titleId} className="text-[16px] font-semibold">{title}</h2>
        {children}
      </div>
    </div>, document.body,
  );
}

// Shape only; colour comes from one of the two variants below so Tailwind never
// has to pick between two background classes on the same button.
export const catalogActionBase = 'h-8 px-3 rounded-[3px] border text-[12.5px] font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-default';
export const catalogActionButton = `${catalogActionBase} border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] hover:border-[var(--accent)]`;
