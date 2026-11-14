import { useFormatCount, useT } from '../i18n/LanguageContext';

interface Props { count: number; onView: (returnTo: HTMLElement) => void; onClose: () => void }

const button = 'px-2.5 py-1 rounded border border-[var(--line)] bg-[var(--panel)] cursor-pointer text-small hover:bg-[var(--panel-2)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Quiet notice after an import or sync that found files worth bundling; it never asks for an answer. */
export function BundleSuggestionHint({ count, onView, onClose }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  if (count <= 0) return null;
  return (
    <div role="region" aria-label={t('bundleSuggestHintAria')} className="pointer-events-auto max-w-[min(90vw,520px)] flex flex-col gap-2 px-3.5 py-2.5 rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] text-[length:var(--font-size-title)] text-[var(--ink)]">
      <p role="status" className="break-words">{formatCount(t('bundleSuggestHint'), count)}</p>
      <div className="flex justify-end gap-2">
        <button type="button" className={button} onClick={event => onView(event.currentTarget)}>{t('bundleSuggestView')}</button>
        <button type="button" className={button} onClick={onClose}>{t('bundleSuggestLater')}</button>
      </div>
    </div>
  );
}
