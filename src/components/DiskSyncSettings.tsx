import { SettingRow } from './SettingsTools';
import { SETTING_DEFAULTS } from '../lib/settingsSession';
import { useEffect, useRef, useState } from 'react';
import { useT, useFormatCount } from '../i18n/LanguageContext';
import { useImportLock } from '../hooks/ImportLockContext';
import { ErrorText } from '../diagnostics/ErrorText';
import { toAppError, type AppError } from '../lib/errors';
import { displayName } from '../lib/displayName';
import { ToggleSwitch } from './ToggleSwitch';
import { Disclosure } from './Disclosure';
import { countKnownCopies, getSyncAutoAdd, getSyncOnStart, listKnownCopies, resetKnownCopies, setSyncAutoAdd, setSyncOnStart, type KnownCopyList } from '../lib/api/diskSync';

const smallButton = 'px-2 py-1 rounded border border-[var(--line)] bg-[var(--panel)] text-small cursor-pointer disabled:opacity-50 disabled:cursor-default hover:border-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Settings for the sync with the disk at start (both off by default). */
export function DiskSyncSettings() {
  const t = useT();
  // The web server has no list of remembered copies; it takes new files in by its own start check and has its own wording.
  const web = import.meta.env.VITE_TARGET === 'web';
  const startLabel = t(web ? 'webSyncOnStartLabel' : 'syncOnStartLabel');
  const addLabel = t(web ? 'webSyncAutoAddLabel' : 'syncAutoAddLabel');
  const count = useFormatCount();
  const { jobActive, lockProps } = useImportLock();
  const [known, setKnown] = useState(0);
  const [list, setList] = useState<KnownCopyList | null>(null);
  const [resetDone, setResetDone] = useState(false);
  const countRef = useRef<HTMLSpanElement>(null);
  const [loaded, setLoaded] = useState(false);
  const [onStart, setOnStart] = useState(false);
  const [autoAdd, setAutoAdd] = useState(false);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(true);
  const [error, setError] = useState<AppError | null>(null);
  useEffect(() => {
    let active = true;
    Promise.all([getSyncOnStart(), getSyncAutoAdd(), web ? Promise.resolve(0) : countKnownCopies()])
      .then(([start, add, copies]) => { if (active) { setLoaded(true); setOnStart(start === true); setAutoAdd(add === true); setKnown(typeof copies === 'number' ? copies : 0); } })
      .catch(e => { if (active) setError(toAppError(e)); })
      .finally(() => { if (active) { setLoading(false); setBusy(false); } });
    return () => { active = false; };
  }, [web]);
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
  // The list is read when it is opened, so it is never older than the count next to it.
  // No busy flag here: disabling the button that was just pressed would drop its focus.
  const toggleList = async () => {
    if (list) { setList(null); return; }
    setError(null);
    try { setList(await listKnownCopies()); } catch (e) { setError(toAppError(e)); }
  };
  // The buttons turn disabled at zero and a disabled button cannot keep focus: park it on the count.
  const reset = () => save(async () => { await resetKnownCopies(); }, () => {
    setKnown(0); setList(null); setResetDone(true);
    queueMicrotask(() => countRef.current?.focus());
  });
  return (
    <div>
      <p className="text-small text-[var(--ink-2)] mb-2">{t('syncSettingsIntro')}</p>
      <SettingRow id="syncStart" value={loaded ? onStart : undefined} label={startLabel} onReset={() => changeOnStart(SETTING_DEFAULTS.syncStart)} disabled={!loaded || loading || busy || jobActive}>
        <div className="flex items-center justify-between gap-2 text-small">
          <span className="font-semibold">{startLabel}</span>
          <ToggleSwitch checked={onStart} label={startLabel} disabled={loading} busy={busy} aria-describedby="sync-on-start-desc" onChange={value => void changeOnStart(value)} />
        </div>
        <p id="sync-on-start-desc" className="mt-1 text-small text-[var(--ink-2)]">{t(web ? 'webSyncOnStartDescription' : 'syncOnStartDescription')}</p>

      </SettingRow>
      <SettingRow id="syncAdd" value={loaded ? autoAdd : undefined} label={addLabel} onReset={() => changeAutoAdd(SETTING_DEFAULTS.syncAdd)} disabled={!loaded || loading || busy || jobActive}>
        <div className="flex items-center justify-between gap-2 text-small mt-3">
          <span className="font-semibold">{addLabel}</span>
          <ToggleSwitch checked={onStart && autoAdd} label={addLabel} disabled={loading || !onStart} busy={busy} aria-describedby={web ? 'sync-auto-add-desc' : 'sync-auto-add-desc sync-auto-add-warning'} onChange={value => void changeAutoAdd(value)} />
        </div>
        <p id="sync-auto-add-desc" className="mt-1 text-small text-[var(--ink-2)]">{t(web ? 'webSyncAutoAddDescription' : 'syncAutoAddDescription')}</p>
        {!web && <p id="sync-auto-add-warning" className="mt-1 text-small text-[var(--ink-2)]">{t('syncAutoAddWarning')}</p>}

      </SettingRow>
      {!web && <>
      <div className="flex flex-wrap min-w-0 items-center gap-2 mt-4 text-small">
        <span id="known-copies-count" ref={countRef} tabIndex={-1} className="flex-auto min-w-0 whitespace-nowrap focus-visible:outline-2 focus-visible:outline-[var(--accent)]">{count(t('kcCount'), known)}</span>
        <button type="button" className={`${smallButton} min-w-0 [overflow-wrap:anywhere]`} disabled={busy || known === 0} aria-expanded={list !== null} aria-controls="known-copies-list" aria-describedby="known-copies-count" onClick={() => void toggleList()}>{t(list ? 'kcHide' : 'kcShow')}</button>
        <button type="button" className={`${smallButton} min-w-0 [overflow-wrap:anywhere]`} disabled={busy || jobActive || known === 0} aria-describedby="known-copies-count known-copies-reset-note" {...lockProps} onClick={() => void reset()}>{t('kcReset')}</button>
      </div>
      <p className="mt-1.5 text-small text-[var(--ink-2)]">{t(known === 0 ? 'kcEmpty' : 'kcHint')}</p>
      {known > 0 && <p id="known-copies-reset-note" className="mt-1 text-small text-[var(--ink-2)]">{t('kcResetNote')}</p>}
      <Disclosure summary={t('kcHowTitle')} className="mt-1.5">{t('kcHowText')}</Disclosure>
      {resetDone && known === 0 && <p role="status" className="mt-1 text-small text-[var(--ink-2)]">{t('kcResetDone')}</p>}
      {list && list.items.length > 0 && <>
        <ul id="known-copies-list" aria-label={t('kcListLabel')} className="mt-2">{list.items.map(item => (
          <li key={item.path} title={displayName(item.path)} className="py-1.5 border-t border-[var(--line)] first:border-t-0 text-small">
            <span className="block font-code [overflow-wrap:anywhere]">{displayName(item.relativePath)}</span>
            {item.copyOfName && <span className="block text-[var(--ink-2)]">{t('kcCopyOf').replace('{name}', displayName(item.copyOfName))}</span>}
          </li>))}</ul>
        {list.total > list.items.length && <p className="mt-1 text-small text-[var(--ink-2)]">{count(t('kcMore'), list.total - list.items.length)}</p>}
      </>}
      </>}
      {error && <p role="alert" className="text-small text-[var(--crit)]"><ErrorText error={error} /></p>}
    </div>
  );
}
