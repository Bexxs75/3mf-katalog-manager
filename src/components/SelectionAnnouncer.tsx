import { useEffect, useRef, useState } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';

/**
 * Invisible live region that tells screen reader users how many models are selected.
 * Rows only expose their own aria-selected state, so without this the effect of
 * Ctrl+A, Space or "clear selection" would be silent. Whole bundle cards in the
 * selection are named too ("2 bundles, 7 models selected").
 */
export function SelectionAnnouncer({ count, bundleCount = 0 }: { count: number; bundleCount?: number }) {
  const t = useT();
  const formatCount = useFormatCount();
  const previous = useRef({ count, bundleCount });
  const [message, setMessage] = useState('');
  useEffect(() => {
    const before = previous.current;
    previous.current = { count, bundleCount };
    if (before.count === count && before.bundleCount === bundleCount) return;
    // Clearing an already empty selection says nothing; the first render is never announced.
    const models = formatCount(t('selectionAnnounce'), count);
    setMessage(count === 0 ? t('selectionCleared') : bundleCount > 0
      ? t('bundleSelectionAnnounce').replace('{bundles}', formatCount(t('bundleSelectionBundles'), bundleCount)).replace('{models}', models)
      : models);
  }, [count, bundleCount, formatCount, t]);
  return <div aria-live="polite" aria-atomic="true" data-selection-announcer className="sr-only">{message}</div>;
}
