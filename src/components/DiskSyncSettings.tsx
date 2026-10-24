import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { getSyncAutoAdd, getSyncOnStart, setSyncAutoAdd, setSyncOnStart } from '../lib/api/diskSync';

const box = 'mt-0.5 shrink-0';

/** Settings for the sync with the disk at start (both off by default). */
export function DiskSyncSettings() {
  const t = useT();
  const [onStart, setOnStart] = useState(false);
  const [autoAdd, setAutoAdd] = useState(false);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    let active = true;
    Promise.all([getSyncOnStart(), getSyncAutoAdd()])
      .then(([start, add]) => { if (active) { setOnStart(start === true); setAutoAdd(add === true); } })
      .catch(e => { if (active) setError(toAppError(e)); })
      .finally(() => { if (active) setBusy(false); });
    return () => { active = false; };
  }, []);
  const save = async (write: () => Promise<unknown>, apply: () => void) => {
    setBusy(true);
    setError(null);
    try { await write(); apply(); } catch (e) { setError(toAppError(e)); } finally { setBusy(false); }
  };
  // Turning the main switch off also clears the automatic take-over, so it does not come back unseen later.
  const changeOnStart = (value: boolean) => save(async () => {
    await setSyncOnStart(value);
    if (!value && autoAdd) await setSyncAutoAdd(false);
  }, () => { setOnStart(value); if (!value) setAutoAdd(false); });
  const changeAutoAdd = (value: boolean) => save(() => setSyncAutoAdd(value), () => setAutoAdd(value));
  return (
    <section aria-labelledby="sync-settings-title" className="mb-4">
      <h3 id="sync-settings-title" className="text-[length:var(--font-size-body)] font-semibold mb-1">{t('syncSettingsTitle')}</h3>
      <p className="text-small text-[var(--ink-2)] mb-2">{t('syncSettingsIntro')}</p>
      <label className="flex gap-2 items-start text-small cursor-pointer">
        <input type="checkbox" className={box} checked={onStart} disabled={busy} aria-describedby="sync-on-start-desc" onChange={e => void changeOnStart(e.target.checked)} />
        <span>{t('syncOnStartLabel')}</span>
      </label>
      <p id="sync-on-start-desc" className="mt-1 ml-6 text-small text-[var(--ink-2)]">{t('syncOnStartDescription')}</p>
      <label className={`flex gap-2 items-start text-small mt-3 ${onStart ? 'cursor-pointer' : 'opacity-60'}`}>
        <input type="checkbox" className={box} checked={onStart && autoAdd} disabled={busy || !onStart} aria-describedby="sync-auto-add-desc sync-auto-add-warning" onChange={e => void changeAutoAdd(e.target.checked)} />
        <span>{t('syncAutoAddLabel')}</span>
      </label>
      <p id="sync-auto-add-desc" className="mt-1 ml-6 text-small text-[var(--ink-2)]">{t('syncAutoAddDescription')}</p>
      <p id="sync-auto-add-warning" className="mt-1 ml-6 text-small text-[var(--ink-2)]">{t('syncAutoAddWarning')}</p>
      {error && <p role="alert" className="text-small text-[var(--danger)]"><ErrorText error={error} /></p>}
    </section>
  );
}
