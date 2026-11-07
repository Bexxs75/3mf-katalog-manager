import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { SettingLoadStatus, SettingSaveStatus, useLoadedSetting } from './SettingState';
import { ToggleSwitch } from './ToggleSwitch';
import { BlockedAutoTagsSettings, READ_ONLY_NOTE_ID } from './BlockedAutoTagsSettings';
import * as api from '../lib/api/autoTags';
import { useCanEditMetadata } from '../lib/webSession';

/** Settings switch: whether the app assigns tags automatically at all. */
export function AutoTaggingSetting() {
  const t = useT();
  const canEdit = useCanEditMetadata();
  // No stored value is not the same as "unknown": the backend answers true then, so only a failed read leaves it open.
  const { value, confirm, state, error: loadError, reload } = useLoadedSetting(api.getAutoTagging);
  const enabled = value === undefined ? null : value !== false;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  const change = async (next: boolean) => {
    setBusy(true); setError(null);
    try {
      await api.setAutoTagging(next);
      if (mounted.current) confirm(next);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };

  return (
    <>
      <div>
        <div className="flex items-center justify-between gap-2 text-small">
          <span className="font-semibold">{t('autoTaggingLabel')}</span>
          <ToggleSwitch checked={enabled === true} label={t('autoTaggingLabel')} disabled={busy || !canEdit || enabled === null} aria-describedby={canEdit ? undefined : READ_ONLY_NOTE_ID} onChange={next => void change(next)} />
        </div>
        <p className="mt-1 text-small text-[var(--ink-2)]">{t('autoTaggingHint')}</p>
        {!canEdit && <p id={READ_ONLY_NOTE_ID} className="mt-1 text-small text-[var(--ink-2)]">{t('settingsReadOnlyNotice')}</p>}
        <SettingLoadStatus state={state} error={loadError} onReload={() => void reload()} className="mt-1.5" />
        <SettingSaveStatus saving={busy} error={error} keptValue className="mt-1.5" />
      </div>
      {/* Stays fully readable when tagging is off: blocks can still be lifted, and they apply again once it is switched on. */}
      <div className="mt-4">
        <BlockedAutoTagsSettings />
        {enabled === false && <p className="mt-1 text-caption text-[var(--ink-3)]">{t('autoTaggingBlockedNote')}</p>}
      </div>
    </>
  );
}
