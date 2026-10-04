import { useModalDialog } from '../hooks/useModalDialog';
import { useId, type ReactNode, type RefObject } from 'react';
import { createPortal } from 'react-dom';

/** Shared keyboard boundary for catalog-only confirmations. */
export function CatalogActionDialog({ title, children, onClose, returnFocus }: {
  title: string;
  children: ReactNode;
  onClose: () => void;
  returnFocus?: RefObject<HTMLElement | null>;
}) {
  const ref = useModalDialog({ open: true, onClose, returnFocus, initialFocus: '[data-initial-focus]' });
  const titleId = useId();
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
