import { useEffect } from 'react';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { displayName } from '../lib/displayName';
import { Icon } from './Icon';
import type { SourceNotice } from '../hooks/useSourceFolderCheck';

const folderName = (path: string) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
const small = 'px-2.5 py-1 rounded-[4px] border border-[var(--line-strong)] bg-transparent text-[var(--ink-2)] text-[length:var(--font-size-meta)] font-semibold cursor-pointer hover:bg-[var(--panel-2)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Notice after a source folder check: a button to look at the files, or a short result. */
export function SourceFolderNotice({ notice, onView, onClose }: { notice: SourceNotice; onView: () => void; onClose: () => void }) {
  const t = useT();
  const count = useFormatCount();
  // The "nothing new" answer is only a status; everything else waits for the user.
  const transient = notice.kind === 'none' && !notice.truncated && notice.skipped === 0 && notice.unreachable === 0;
  useEffect(() => {
    if (!transient) return;
    const timer = setTimeout(onClose, 6000);
    return () => clearTimeout(timer);
  }, [transient, onClose]);

  // A click removes the button that had focus; hand focus to the catalog. The timer must not do this.
  const dismiss = () => { document.querySelector<HTMLElement>('[data-catalog-main]')?.focus(); onClose(); };

  let text: string;
  if (notice.kind === 'found') {
    text = notice.folderCount === 1
      ? count(t('sfFoundOne'), notice.count).replace('{name}', displayName(folderName(notice.folderPath)))
      : count(t('sfFoundMany'), notice.count).replace('{folders}', String(notice.folderCount));
  } else if (notice.kind === 'none') text = t('sfFoundNone');
  else if (notice.kind === 'noFolders') text = t('sfNoFolders');
  else text = '';
  const extras = notice.kind === 'found' || notice.kind === 'none' ? [
    ...(notice.truncated ? [t('sfTruncated')] : []),
    ...(notice.skipped > 0 ? [count(t('sfSkippedEntries'), notice.skipped)] : []),
    ...(notice.unreachable > 0 ? [count(t('sfUnreachableFolders'), notice.unreachable)] : []),
  ] : [];

  return (
    <div role={notice.kind === 'error' ? 'alert' : 'status'} className="pointer-events-auto max-w-[min(90vw,480px)] flex items-start gap-2.5 px-3.5 py-2.5 rounded-[4px] border border-[var(--line)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <div className="flex min-w-0 flex-col gap-1 [overflow-wrap:anywhere]">
        {notice.kind === 'error' ? <span className="text-[var(--crit)]"><ErrorText error={notice.error} /></span> : <span>{text}</span>}
        {extras.length > 0 && <ul className="text-[length:var(--font-size-meta)] text-[var(--ink-3)]">{extras.map(line => <li key={line}>{line}</li>)}</ul>}
        {notice.kind === 'found' && (
          <div className="flex gap-2">
            <button type="button" className={small} onClick={onView}>{t('sfView')}</button>
            <button type="button" className={small} onClick={dismiss}>{t('sfLater')}</button>
          </div>
        )}
      </div>
      {notice.kind !== 'found' && (
        <button type="button" aria-label={t('sfDismiss')} title={t('sfDismiss')} onClick={dismiss}
          className="w-5 h-5 shrink-0 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--panel-2)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
          <Icon name="close" size={14} />
        </button>
      )}
    </div>
  );
}
