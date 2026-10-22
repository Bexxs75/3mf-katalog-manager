import { useEffect, useRef, useState } from 'react';
import { useFormatCount, useT } from '../i18n/LanguageContext';

/**
 * Invisible live region that tells screen reader users how many models are selected.
 * Rows only expose their own aria-selected state, so without this the effect of
 * Ctrl+A, Space or "clear selection" would be silent.
 */
export function SelectionAnnouncer({ count }: { count: number }) {
  const t = useT();
  const formatCount = useFormatCount();
  const previous = useRef(count);
  const [message, setMessage] = useState('');
  useEffect(() => {
    const before = previous.current;
    previous.current = count;
    if (before === count) return;
    // Clearing an already empty selection says nothing; the first render is never announced.
    setMessage(count > 0 ? formatCount(t('selectionAnnounce'), count) : t('selectionCleared'));
  }, [count, formatCount, t]);
  return <div aria-live="polite" aria-atomic="true" data-selection-announcer className="sr-only">{message}</div>;
}
