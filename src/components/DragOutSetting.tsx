import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import * as api from '../lib/api/fileDrag';
import { ToggleSwitch } from './ToggleSwitch';
import { ErrorText } from '../diagnostics/ErrorText';

/** Prototype P5-7: switch for dragging files out of the window (off by default; no drag handle yet). */
export function DragOutSetting() {
  const t = useT();
  const [enabled, setEnabled] = useState<boolean | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    api.getDragOutEnabled().then((v) => { if (mounted.current) setEnabled(v); })
      .catch((e) => { if (mounted.current) setError(toAppError(e)); });
    return () => { mounted.current = false; };
  }, []);
  if (enabled === null && !error) return null;
  const change = async (next: boolean) => {
    setError(null);
    try {
      await api.setDragOutEnabled(next);
      if (mounted.current) setEnabled(next);
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
    }
  };
  return (
    <section className="mt-4 p-2 border border-[var(--line)] rounded text-small">
      <div className="flex items-center justify-between gap-2">
        <span>
          <span className="font-semibold">{t('dgSwitchLabel')}</span>
          <span className="block text-caption text-[var(--ink-3)]">{t('dgSwitchHint')}</span>
        </span>
        <ToggleSwitch checked={enabled === true} label={t('dgSwitchLabel')} disabled={enabled === null} onChange={(v) => void change(v)} />
      </div>
      {error && <div role="alert" className="mt-1 text-[var(--crit)]"><ErrorText error={error} /></div>}
    </section>
  );
}
