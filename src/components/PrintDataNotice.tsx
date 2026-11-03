import { useFormatCount, useT } from '../i18n/LanguageContext';

interface Props {
  /** Files with a missing value that are held back. */
  hidden: number;
  /** Files with a missing value that are shown anyway. */
  shownUnknown: number;
  includeUnknown: boolean;
  onIncludeUnknownChange: (include: boolean) => void;
}

/**
 * Says how many files the print data filter left out because they lack a value, and lets the
 * user show them. One button for both states, so it keeps the focus when it is pressed.
 */
export function PrintDataNotice({ hidden, shownUnknown, includeUnknown, onIncludeUnknownChange }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const count = includeUnknown ? shownUnknown : hidden;
  if (count === 0) return null;
  return (
    <div role="status" data-print-data-notice className="flex-none flex flex-wrap items-center gap-x-3 gap-y-1 px-4 py-2 border-b border-[var(--line)] bg-[var(--panel-2)] text-small">
      <div className="min-w-0 flex-1">
        <strong className="font-semibold">{formatCount(includeUnknown ? t('pfShown') : t('pfHidden'), count)}</strong>{' '}
        <span className="text-[var(--ink-2)]">{includeUnknown ? t('pfShownDetail') : t('pfHiddenDetail')}</span>
      </div>
      <button type="button" onClick={() => onIncludeUnknownChange(!includeUnknown)}
        className="flex-none px-2.5 py-1 rounded-[5px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)]">
        {includeUnknown ? t('pfHide') : t('pfShow')}
      </button>
    </div>
  );
}
