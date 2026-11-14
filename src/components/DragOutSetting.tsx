import { setNativeDragEnabled } from '../lib/nativeDrag';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import * as api from '../lib/api/fileDrag';
import { ToggleSwitch } from './ToggleSwitch';
import { isWindowsPlatform } from '../lib/imageDrop';
import { SettingLoadStatus, SettingSaveStatus, useLoadedSetting } from './SettingState';

/** Experimental native dragging is opt-in. */
export function DragOutSetting() {
  const t = useT();
  const { value, confirm, state, error: loadError, reload } = useLoadedSetting(api.getDragOutEnabled);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const enabled = value === undefined ? null : value;
  const change = async (next: boolean) => {
    // One write at a time: a second click while the first is running would queue a competing write.
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await api.setDragOutEnabled(next);
      if (mounted.current) confirm(next);
      setNativeDragEnabled(next);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
    } finally {
      if (mounted.current) setBusy(false);
    }
  };
  // The platform never changes while the app runs.
  const windows = useRef(isWindowsPlatform()).current;
  return (
    <div>
      <div className="flex items-center justify-between gap-2 text-small">
        <span>
          <span className="font-semibold">{t('dgSwitchLabel')}</span>
          <span id="drag-out-hint" className="block text-caption text-[var(--ink-3)]">{t('dgSwitchHint')}</span>
        </span>
        <ToggleSwitch
          checked={enabled === true}
          label={t('dgSwitchLabel')}
          disabled={state !== 'ready' || enabled === null} busy={busy}
          onChange={(v) => void change(v)}
          aria-describedby={windows ? 'drag-out-hint drag-out-experimental drag-out-windows' : 'drag-out-hint drag-out-experimental'}
        />
      </div>
      <SettingLoadStatus keptValue={value !== undefined && value !== null} state={state} error={loadError} onReload={() => void reload()} className="mt-1" />
      <SettingSaveStatus saving={busy} error={error} keptValue className="mt-1" />
      <p id="drag-out-experimental" className="mt-2 text-caption text-[var(--ink-3)]">{t('dgExperimentalNote')}</p>
      {windows && (
        <div id="drag-out-windows" className="mt-2 text-small">
          <p className="p-2 border border-[var(--warn)] rounded text-caption"><b>{t('dgWinLabel')}</b> {t('dgWinText')}</p>
          <details className="mt-1.5">
            <summary className="cursor-pointer text-small text-[var(--ink-2)]">{t('dgWinSummary')}</summary>
            <div className="mt-1 text-caption text-[var(--ink-2)]">
              <b>{t('dgWinStepsTitle')}</b>
              <ol className="list-decimal pl-5 mt-1 space-y-0.5">
                <li>{t('dgWinStep1')}</li>
                <li>{t('dgWinStep2')}</li>
                <li>{t('dgWinStep3')}</li>
                <li>{t('dgWinStep4')}</li>
                <li>{t('dgWinStep5')}</li>
              </ol>
              <p className="mt-1">{t('dgWinUipi')}</p>
            </div>
          </details>
        </div>
      )}
    </div>
  );
}
