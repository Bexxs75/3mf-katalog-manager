import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { ToggleSwitch } from './ToggleSwitch';
import { BlockedAutoTagsSettings } from './BlockedAutoTagsSettings';
import * as api from '../lib/api/autoTags';
import { isReadOnly } from '../lib/readOnly';
import { useCanEditMetadata } from '../lib/webSession';

/** Settings switch: whether the app assigns tags automatically at all. */
export function AutoTaggingSetting() {
  const t = useT();
  // In the web catalog tab the switch is the first block, so it needs no top margin.
  const web = isReadOnly();
  const canEdit = useCanEditMetadata();
  const [enabled, setEnabled] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  // A failed read leaves the default (on), which is what the backend assumes without a stored value.
  useEffect(() => {
    api.getAutoTagging().then(on => { if (mounted.current) setEnabled(on !== false); }).catch(e => console.error('Reading the automatic tagging setting failed', e));
  }, []);

  const change = async (next: boolean) => {
    setBusy(true); setError(null);
    try {
      await api.setAutoTagging(next);
      if (mounted.current) setEnabled(next);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  return (
    <>
      <div className={web ? undefined : 'mt-4'}>
        <div className="flex items-center justify-between gap-2 text-small">
          <span className="font-semibold">{t('autoTaggingLabel')}</span>
          <ToggleSwitch checked={enabled} label={t('autoTaggingLabel')} disabled={busy || !canEdit} onChange={next => void change(next)} />
        </div>
        <p className="mt-1 text-small text-[var(--ink-2)]">{t('autoTaggingHint')}</p>
        {error && (
          <div role="alert" className="mt-1.5 font-medium tabular-nums text-caption text-[var(--accent)] break-words">
            {t('autoTaggingError')} <ErrorText error={error} />
          </div>
        )}
      </div>
      {/* Dimmed, not disabled: the list still applies if the switch is turned back on, and blocks can still be lifted. */}
      <div className={enabled ? undefined : 'opacity-[0.45]'}>
        <BlockedAutoTagsSettings />
        {!enabled && <p className="mt-1 text-caption text-[var(--ink-3)]">{t('autoTaggingBlockedNote')}</p>}
      </div>
    </>
  );
}
