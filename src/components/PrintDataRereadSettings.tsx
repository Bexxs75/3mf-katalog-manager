import { useId, useRef } from 'react';
import { useT } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { usePrintDataReread } from '../hooks/usePrintDataReread';
import { useCanEditMetadata } from '../lib/webSession';
import { PrintDataRereadJob } from './PrintDataRereadJob';

/** Settings block (catalog tab): reads the print data of all 3MF and G-code files that were not read yet. */
export function PrintDataRereadSettings() {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const { jobActive } = useImportLock();
  const { state, start } = usePrintDataReread();
  const trigger = useRef<HTMLButtonElement>(null);
  const reasonId = useId();
  const running = state.status === 'running' || state.status === 'cancelling';
  // The reason is named in the tooltip and in a description for keyboard and screen reader users. Missing
  // write access is also written out on the page, because a tooltip alone does not reach touch users.
  const reason = !canEdit ? t('settingsReadOnlyNotice') : running ? t('printDataRereadBusy') : jobActive ? t('importLockedHint') : null;

  return (
    <div>
      <p className="text-small text-[var(--ink-2)] mb-2">{t('printDataRereadIntro')}</p>
      <button
        ref={trigger} type="button"
        onClick={() => start('settings')}
        disabled={reason !== null} title={reason ?? undefined} aria-describedby={reason ? reasonId : undefined}
        className="h-7 w-full rounded-[3px] border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-40 disabled:cursor-not-allowed disabled:hover:border-[var(--line-strong)] disabled:hover:text-[var(--ink-2)]"
      >
        {t('printDataRereadAll')}
      </button>
      {reason && <span id={reasonId} className={canEdit ? 'sr-only' : 'mt-1.5 block text-small text-[var(--ink-2)]'}>{reason}</span>}
      <PrintDataRereadJob origin="settings" returnFocus={trigger} />
    </div>
  );
}
