import { useEffect, useRef } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { tagLabel } from '../lib/autoTags';
import { ErrorText } from '../diagnostics/ErrorText';
import type { AppError } from '../lib/errors';
import type { AutoTagBlockNotice } from '../hooks/useAutoTagBlock';

interface Props {
  notice: AutoTagBlockNotice | null;
  error: AppError | null;
  onUndo: () => void;
}

/**
 * Feedback after "remove and do not assign automatically again", with the way back.
 * The chip that held focus is gone by then, so focus moves to the undo button; after
 * the undo it moves to the message so it is not left on a removed button.
 */
export function AutoTagBlockStatus({ notice, error, onUndo }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const statusRef = useRef<HTMLDivElement>(null);
  const undoRef = useRef<HTMLButtonElement>(null);
  const tag = notice?.tag;
  const undone = notice?.undone;

  useEffect(() => {
    if (tag === undefined) return;
    (undone ? statusRef.current : undoRef.current)?.focus();
  }, [tag, undone]);

  const label = notice ? `#${tagLabel(notice.tag, language)}` : '';
  return (
    <>
      {notice && (
        <div ref={statusRef} role="status" tabIndex={-1} className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-caption text-[var(--ink-2)] outline-none">
          {notice.undone
            ? <span>{t('autoTagUnblockedNotice').replace('{label}', label)}</span>
            : <>
              <span>{t('autoTagBlockedNotice').replace('{label}', label)}</span>
              <button ref={undoRef} type="button" onClick={onUndo}
                className="px-2 py-0.5 rounded border border-[var(--line)] bg-[var(--panel)] text-caption font-semibold text-[var(--ink)] cursor-pointer hover:border-[var(--accent)]">
                {t('undoButton')}
              </button>
            </>}
        </div>
      )}
      {error && (
        <div role="alert" className="mt-2 font-medium tabular-nums text-caption text-[var(--accent)] break-words">
          {t('autoTagBlockError')} <ErrorText error={error} />
        </div>
      )}
    </>
  );
}
