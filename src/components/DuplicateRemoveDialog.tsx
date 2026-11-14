import { useId } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { useModalDialog } from '../hooks/useModalDialog';

interface Props {
  /** Models that go to the trash. */
  count: number;
  busy: boolean;
  /** Text of a failed attempt; the dialog stays open so the user can try again or cancel. */
  error: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}

const buttonBase = 'h-8 px-3 rounded-[3px] border text-[length:var(--font-size-title)] font-semibold focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';

/**
 * Asks before the deselected duplicates go to the app trash. The focus starts on "Cancel": removing
 * is the step that costs something, so it is never one stray Enter away.
 */
export function DuplicateRemoveDialog({ count, busy, error, onConfirm, onCancel }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { lockProps } = useImportLock();
  const titleId = useId();
  const dialogRef = useModalDialog({ open: true, onClose: onCancel, busy, initialFocus: '[data-initial-focus]' });
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-3">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}
        className="w-[440px] max-w-full max-h-full overflow-y-auto bg-[var(--panel)] text-[var(--ink)] border border-[var(--line-strong)] rounded-[10px] shadow-[var(--shadow)] px-4 py-4">
        <h2 id={titleId} className="m-0 mb-2 text-[length:var(--font-size-title)] font-bold">{t('dvConfirmTitle')}</h2>
        <p className="m-0 mb-1.5 text-body font-medium">{formatCount(t('trashFolderModels'), count)}</p>
        <p className="m-0 mb-1.5 text-small text-[var(--ink-2)]">{t('dvConfirmKeep')}</p>
        <p className="m-0 mb-3 text-small text-[var(--ink-2)]">{t('dvConfirmLoss')}</p>
        {error && <p role="alert" className="m-0 mb-3 text-small text-[var(--crit)] break-words">{error}</p>}
        <div className="flex justify-end gap-2">
          <button type="button" data-initial-focus onClick={onCancel} disabled={busy}
            className={`${buttonBase} border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-40 disabled:cursor-not-allowed`}>
            {t('cancel')}
          </button>
          <button type="button" onClick={onConfirm} disabled={busy} {...lockProps}
            className={`${buttonBase} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] cursor-pointer hover:bg-[var(--panel-2)] hover:text-[var(--ink)] disabled:opacity-40 disabled:cursor-not-allowed`}>
            {t('dvConfirmRemove')}
          </button>
        </div>
      </div>
    </div>
  );
}
