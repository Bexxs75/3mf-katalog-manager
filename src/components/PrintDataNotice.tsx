import { useFormatCount, useT } from '../i18n/LanguageContext';
import { useCanEditMetadata } from '../lib/webSession';

interface Props {
  /** Files with a missing value that are held back. */
  hidden: number;
  /** Files with a missing value that are shown anyway. */
  shownUnknown: number;
  /** Of the held-back files, those whose print data were never read (a read could fill them in). */
  unread?: number;
  /** Opens the place where unread files are read; without it (or without the right to change the catalog) the line only names the way. */
  onReread?: () => void;
  includeUnknown: boolean;
  onIncludeUnknownChange: (include: boolean) => void;
}

const buttonClass = 'flex-none px-2.5 py-1 rounded-[5px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';

/**
 * Says how many files the print data filter left out because they lack a value, and lets the
 * user show them. One button for both states, so it keeps the focus when it is pressed. Some of the
 * held-back files may only lack the value because they were never read: the second line says so.
 */
export function PrintDataNotice({ hidden, shownUnknown, unread = 0, onReread, includeUnknown, onIncludeUnknownChange }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const canEdit = useCanEditMetadata();
  const count = includeUnknown ? shownUnknown : hidden;
  if (count === 0) return null;
  const showUnread = !includeUnknown && unread > 0;
  return (
    <div role="status" data-print-data-notice className="flex-none flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2 border-b border-[var(--line)] bg-[var(--panel-2)] text-small">
      <div className="min-w-0 flex-1">
        <div>
          <strong className="font-semibold">{formatCount(includeUnknown ? t('pfShown') : t('pfHidden'), count)}</strong>{' '}
          <span className="text-[var(--ink-2)]">{includeUnknown ? t('pfShownDetail') : t('pfHiddenDetail')}</span>
        </div>
        {showUnread && (
          <div data-print-data-unread className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-[var(--ink-2)]">
            <span>{formatCount(t('pfUnread'), unread)}</span>
            {onReread && canEdit
              ? <button type="button" onClick={onReread} className={buttonClass}>{t('pfUnreadReread')}</button>
              : <span>{t('pfUnreadWhere')}</span>}
          </div>
        )}
      </div>
      <button type="button" onClick={() => onIncludeUnknownChange(!includeUnknown)} className={buttonClass}>
        {includeUnknown ? t('pfHide') : t('pfShow')}
      </button>
    </div>
  );
}
