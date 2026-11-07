import { setNativeDragEnabled } from '../lib/nativeDrag';
import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import * as api from '../lib/api/fileDrag';
import { ToggleSwitch } from './ToggleSwitch';
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
  return (
    <section className="mt-4 p-2 border border-[var(--line)] rounded text-small">
      <div className="flex items-center justify-between gap-2">
        <span>
          <span className="font-semibold">{t('dgSwitchLabel')}</span>
          <span className="block text-caption text-[var(--ink-3)]">{t('dgSwitchHint')}</span>
        </span>
        <ToggleSwitch checked={enabled === true} label={t('dgSwitchLabel')} disabled={enabled === null || busy} onChange={(v) => void change(v)} />
      </div>
      <SettingLoadStatus state={state} error={loadError} onReload={() => void reload()} className="mt-1" />
      <SettingSaveStatus saving={busy} error={error} keptValue className="mt-1" />
    </section>
  );
}
