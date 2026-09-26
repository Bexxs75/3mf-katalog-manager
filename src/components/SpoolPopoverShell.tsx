import { useMemo, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { useT } from '../i18n/LanguageContext';
import { useAnchoredPopup } from '../hooks/useAnchoredPopup';

export const fieldClass =
  'w-full min-w-0 h-7 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel-2)] text-[var(--ink)] text-[12.5px] outline-0 focus:border-[var(--accent)]';
export const buttonClass = 'h-[30px] px-3 rounded-md text-[12.5px] font-semibold cursor-pointer border';

interface Props {
  /** The button the popover hangs on. */
  anchor: HTMLElement;
  onClose: () => void;
  minWidth: number;
  title: string;
  subtitle: string;
  error: string | null;
  busy: boolean;
  submitLabel: ReactNode;
  onSubmit: () => void;
  /**
   * Additional keyboard handling (e.g. Enter in the input submits).
   * Escape is already handled by the shell and always closes,
   * regardless of whether this handler is set.
   */
  onKeyDown?: (e: ReactKeyboardEvent) => void;
  /** Form content between subtitle and error display. */
  children: ReactNode;
}

/**
 * Shared shell of the inventory popovers ("Restock", "− Usage"): portal
 * window positioned via useAnchoredPopup with title, form (children), error
 * display and cancel/submit. Escape, "Cancel" and clicking outside always
 * close without submitting.
 */
export function SpoolPopoverShell({
  anchor,
  onClose,
  minWidth,
  title,
  subtitle,
  error,
  busy,
  submitLabel,
  onSubmit,
  onKeyDown,
  children,
}: Props) {
  const t = useT();
  // Stable ref wrapper because useAnchoredPopup expects a RefObject.
  const anchorRef = useMemo(() => ({ current: anchor }), [anchor]);
  const { popupRef, style } = useAnchoredPopup<HTMLElement, HTMLDivElement>(anchorRef, true, onClose, minWidth);

  const cancel = () => {
    onClose();
    anchor.focus();
  };

  const handleKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      cancel();
      return;
    }
    onKeyDown?.(e);
  };

  if (!style) return null;

  return createPortal(
    <div
      ref={popupRef}
      role="dialog"
      aria-label={title}
      style={style}
      onKeyDown={handleKeyDown}
      className="z-[60] rounded-[10px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)] p-3.5 flex flex-col gap-2.5"
    >
      <div>
        <h3 className="text-[13px] font-bold m-0">{title}</h3>
        <p className="text-[11.5px] text-[var(--ink-3)] m-0">{subtitle}</p>
      </div>

      {children}

      {error && (
        <div role="alert" className="text-[11.5px] text-[var(--accent)] break-words">
          {error}
        </div>
      )}

      <div className="flex justify-end gap-1.5">
        <button type="button" onClick={cancel} className={`${buttonClass} border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)]`}>
          {t('cancel')}
        </button>
        <button
          type="button"
          onClick={onSubmit}
          disabled={busy}
          className={`${buttonClass} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          {submitLabel}
        </button>
      </div>
    </div>,
    document.body,
  );
}
