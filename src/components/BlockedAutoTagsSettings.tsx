import { useCallback, useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { tagLabel, otherTagNames } from '../lib/autoTags';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import * as api from '../lib/api/autoTags';

/** Settings section: automatic tags the app no longer assigns, each with a way to lift the block. */
export function BlockedAutoTagsSettings() {
  const t = useT();
  const { language } = useLanguage();
  const [tags, setTags] = useState<string[] | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [loadFailed, setLoadFailed] = useState(false);
  const mounted = useRef(true);
  const sectionRef = useRef<HTMLDivElement>(null);
  const emptyRef = useRef<HTMLParagraphElement>(null);
  // Focus lands on the next row's button after a row disappears (or on the empty text after the last).
  const focusIndex = useRef<number | null>(null);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const load = useCallback(async () => {
    try {
      const list = await api.listBlockedAutoTags();
      if (mounted.current) { setTags(Array.isArray(list) ? list : []); setLoadFailed(false); }
    } catch {
      if (mounted.current) { setTags([]); setLoadFailed(true); }
    }
  }, []);
  useEffect(() => { void load(); }, [load]);

  useEffect(() => {
    if (focusIndex.current === null || tags === null) return;
    const buttons = sectionRef.current?.querySelectorAll<HTMLButtonElement>('button[data-unblock]');
    const target = buttons && buttons.length > 0 ? buttons[Math.min(focusIndex.current, buttons.length - 1)] : emptyRef.current;
    target?.focus();
    focusIndex.current = null;
  }, [tags]);

  const unblock = async (tag: string, index: number) => {
    setError(null);
    try {
      await api.unblockAutoTag(tag);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
      return;
    }
    focusIndex.current = index;
    await load();
  };

  return (
    <div ref={sectionRef} className="mt-4">
      <div className="text-[length:var(--font-size-body)] font-semibold mb-1">{t('blockedAutoTagsTitle')}</div>
      <p className="text-caption text-[var(--ink-3)] mb-2">{t('blockedAutoTagsHint')}</p>
      {loadFailed && (
        <div role="alert" className="mb-1.5 text-caption text-[var(--accent)]">{t('blockedAutoTagsLoadError')}</div>
      )}
      {tags !== null && tags.length === 0 && !loadFailed && (
        <p ref={emptyRef} tabIndex={-1} className="text-caption text-[var(--ink-3)] outline-none">{t('blockedAutoTagsEmpty')}</p>
      )}
      {tags !== null && tags.length > 0 && (
        <ul className="m-0 p-0 list-none">
          {tags.map((tag, index) => {
            const label = `#${tagLabel(tag, language)}`;
            const others = otherTagNames(tag, language);
            return (
              <li key={tag} className="flex items-center justify-between gap-2 py-1.5 border-b border-[var(--line)]">
                <span className="min-w-0 break-words text-small">
                  <span className="font-medium">{label}</span>
                  {others.length > 0 && (
                    <span className="block text-caption text-[var(--ink-3)]">
                      {t('blockedAutoTagAlso').replace('{names}', others.join(', '))}
                    </span>
                  )}
                </span>
                <button
                  type="button" data-unblock
                  aria-label={t('blockedAutoTagUnblockAria').replace('{label}', label)}
                  onClick={() => void unblock(tag, index)}
                  className="flex-none px-2 py-1 rounded border border-[var(--line)] bg-[var(--panel)] text-small cursor-pointer hover:border-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
                >
                  {t('blockedAutoTagUnblock')}
                </button>
              </li>
            );
          })}
        </ul>
      )}
      {error && (
        <div role="alert" className="mt-1.5 font-medium tabular-nums text-caption text-[var(--accent)] break-words">
          {t('autoTagBlockError')} <ErrorText error={error} />
        </div>
      )}
    </div>
  );
}
