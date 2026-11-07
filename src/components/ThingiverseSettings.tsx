import { useEffect, useId, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { SettingLoadStatus, SettingSaveStatus, useLoadedSetting } from './SettingState';
import { isReadOnly } from '../lib/readOnly';
import * as api from '../lib/api/thingiverse';
import type { ThingiverseSettings as Settings } from '../types';
import { ToggleSwitch } from './ToggleSwitch';
import { ExternalLink } from './ExternalLink';
import { Icon } from './Icon';
import { CatalogActionDialog, catalogActionButton } from './CatalogActionDialog';

const APP_CREATE_URL = 'https://www.thingiverse.com/apps/create';
/** From this many requests on, the day's counter warns. */
const WARN_AT = 800;

const primaryButton = 'h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-small font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-default';
const smallButton = 'h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)]';

/**
 * Settings block of the Thingiverse area (desktop only): the switch, the user's own key,
 * the day's counter and the privacy note. The key is typed in here and never comes back:
 * the backend only reports that one exists and a mask with its last four characters.
 */
export function ThingiverseSettings({ onEnabledChange }: { onEnabledChange: (enabled: boolean) => void }) {
  // The web build has no such area, and the hook must not run there (no backend to ask).
  if (isReadOnly()) return null;
  return <ThingiverseSettingsBlock onEnabledChange={onEnabledChange} />;
}

function ThingiverseSettingsBlock({ onEnabledChange }: { onEnabledChange: (enabled: boolean) => void }) {
  const t = useT();
  const { value: settings, confirm, state: loadState, error: loadError, reload: reloadSettings } = useLoadedSetting<Settings>(api.getThingiverseSettings);
  const [dialog, setDialog] = useState<'on' | 'remove' | null>(null);
  const [token, setToken] = useState('');
  const [show, setShow] = useState(false);
  const [replacing, setReplacing] = useState(false);
  const [checking, setChecking] = useState(false);
  const [saving, setSaving] = useState(false);
  const [switchError, setSwitchError] = useState<AppError | null>(null);
  const [userName, setUserName] = useState<string | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [howOpen, setHowOpen] = useState(false);
  const mounted = useRef(true);
  const howId = useId();
  const fieldId = useId();
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);

  // Refresh after a key change: a failed read keeps the last confirmed settings and says so.
  const reload = () => api.getThingiverseSettings()
    .then(s => { if (mounted.current) confirm(s); })
    .catch(e => { if (mounted.current) setError(toAppError(e)); });

  if (!settings) {
    return (
      <section className="mt-4 mb-4 p-2 border border-[var(--line)] rounded text-small">
        <div className="flex items-center gap-2">
          <h3 className="font-semibold">{t('tvSettingsTitle')}</h3>
          <span className="px-1.5 rounded-full border border-[var(--line-strong)] text-caption text-[var(--ink-2)]">{t('tvExperimental')}</span>
        </div>
        <SettingLoadStatus state={loadState} error={loadError} onReload={() => void reloadSettings()} className="mt-1" />
      </section>
    );
  }

  const setEnabled = async (next: boolean) => {
    if (saving) return;
    setSaving(true);
    setSwitchError(null);
    try {
      await api.setThingiverseEnabled(next);
      if (!mounted.current) return;
      confirm({ ...settings, enabled: next });
      if (!next) { setToken(''); setReplacing(false); setUserName(null); }
      onEnabledChange(next);
    } catch (e) {
      if (mounted.current) setSwitchError(toAppError(e));
    } finally {
      if (mounted.current) setSaving(false);
    }
  };

  const check = async () => {
    setChecking(true); setError(null); setUserName(null);
    try {
      const result = await api.testThingiverseToken(token);
      if (!mounted.current) return;
      // The field is emptied as soon as the key is stored: it never shows it again.
      setToken(''); setShow(false); setReplacing(false); setUserName(result.userName ?? '');
      await reload();
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
    } finally {
      if (mounted.current) setChecking(false);
    }
  };

  const remove = async () => {
    setDialog(null); setError(null);
    try {
      await api.removeThingiverseToken();
      if (!mounted.current) return;
      setUserName(null); setReplacing(false);
      await reload();
    } catch (e) {
      if (mounted.current) setError(toAppError(e));
    }
  };

  const { enabled, hasToken } = settings;
  const showField = enabled && (!hasToken || replacing);
  return (
    <section className="mt-4 mb-4 p-2 border border-[var(--line)] rounded text-small" aria-labelledby={`${fieldId}-h`}>
      <div className="flex items-center gap-2">
        <h3 id={`${fieldId}-h`} className="font-semibold">{t('tvSettingsTitle')}</h3>
        <span className="px-1.5 rounded-full border border-[var(--line-strong)] text-caption text-[var(--ink-2)]">{t('tvExperimental')}</span>
      </div>
      <p className="mt-1 text-[var(--ink-2)]">{t('tvSettingsIntro')}</p>
      <div className="mt-2 flex items-center justify-between gap-2">
        <span>
          <span className="font-semibold">{t('tvSwitchLabel')}</span>
          <span className="block text-caption text-[var(--ink-3)]">{t('tvSwitchHint')}</span>
        </span>
        <ToggleSwitch checked={enabled} label={t('tvSwitchLabel')} disabled={saving} onChange={next => { if (next) setDialog('on'); else void setEnabled(false); }} />
      </div>
      <SettingSaveStatus saving={saving} error={switchError} keptValue className="mt-1" />

      {enabled && (
        <div className="mt-3 space-y-2">
          <label htmlFor={showField ? fieldId : undefined} className="block font-semibold">{t('tvTokenLabel')}</label>
          {hasToken && (
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-code tabular-nums" aria-label={t('tvTokenMaskLabel')}>{settings.tokenMask}</span>
              <span className="text-[var(--good)]">{t('tvTokenStored')}</span>
              <button type="button" className={smallButton} onClick={() => { setReplacing(true); setUserName(null); }}>{t('tvTokenReplace')}</button>
              <button type="button" className={`${smallButton} !border-[var(--crit)] !text-[var(--crit)]`} onClick={() => setDialog('remove')}>{t('tvTokenRemove')}</button>
            </div>
          )}
          {hasToken && !replacing && <p className="text-caption text-[var(--ink-3)]">{t('tvTokenNeverShown')}</p>}
          {showField && (
            <form className="flex flex-wrap items-center gap-2" onSubmit={e => { e.preventDefault(); if (token.trim() && !checking) void check(); }}>
              <input
                id={fieldId}
                type={show ? 'text' : 'password'}
                value={token}
                onChange={e => setToken(e.target.value)}
                autoComplete="off"
                spellCheck={false}
                placeholder={t('tvTokenPlaceholder')}
                aria-invalid={error !== null}
                className="flex-1 min-w-[160px] h-8 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]"
              />
              <button type="button" className={smallButton} aria-pressed={show} onClick={() => setShow(s => !s)}>{show ? t('tvTokenHide') : t('tvTokenShow')}</button>
              <button type="submit" className={primaryButton} disabled={checking || !token.trim()}>{checking ? t('tvTokenChecking') : t('tvTokenCheck')}</button>
            </form>
          )}
          {userName !== null && <p role="status" className="text-[var(--good)]">{t('tvTokenValid').replace('{0}', userName)}</p>}
          {error && <p role="alert" className="text-[var(--crit)] break-words"><ErrorText error={error} /></p>}
          <p className="p-2 rounded border border-[var(--line)] bg-[var(--panel-2)] text-caption">{t('tvTokenNote')}</p>
          <p className="text-caption text-[var(--ink-2)]">{t('tvKeptOnRestore')}</p>

          <div>
            <button type="button" aria-expanded={howOpen} aria-controls={howId} onClick={() => setHowOpen(o => !o)}
              className="text-[var(--accent)] cursor-pointer hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
              <span aria-hidden="true" className="inline-block align-middle mr-1"><Icon name="chevron" size={12} className={howOpen ? '' : '-rotate-90'} /></span>{t('tvHowTitle')}
            </button>
            {howOpen && (
              <div id={howId} className="mt-1">
                <ol className="list-decimal pl-5 space-y-1">
                  <li>{t('tvHowStep1')}</li>
                  <li>{t('tvHowStep2')} <ExternalLink url={APP_CREATE_URL} label={t('tvHowOpen')} className="text-[var(--accent)] underline" /></li>
                  <li>{t('tvHowStep3')}</li>
                  <li>{t('tvHowStep4')}</li>
                  <li>{t('tvHowStep5')}</li>
                </ol>
                <p className="mt-1 text-caption text-[var(--ink-3)]">{t('tvHowMayChange')}</p>
              </div>
            )}
          </div>

          {hasToken && (
            <p className="text-[var(--ink-2)]">
              <span className="tabular-nums">{t('tvUsageToday').replace('{0}', String(settings.usageToday)).replace('{1}', String(settings.dailyLimit))}</span>{' '}
              <span className="text-caption text-[var(--ink-3)]">({t('tvUsageEstimate')})</span>
              {settings.usageToday >= WARN_AT && <span role="status" className="block text-[var(--warn)]">{t('tvUsageWarn')}</span>}
            </p>
          )}
        </div>
      )}
      <p className="mt-3 p-2 rounded border border-[var(--line)] text-caption">{t('tvPrivacy')}</p>

      {dialog === 'on' && (
        <CatalogActionDialog title={t('tvEnableTitle')} onClose={() => setDialog(null)}>
          <p>{t('tvEnableIntro')}</p>
          <ul className="list-disc pl-5 space-y-2">
            {(['tvEnablePoint1', 'tvEnablePoint2', 'tvEnablePoint3', 'tvEnablePoint4'] as const).map(key => <li key={key}>{t(key)}</li>)}
          </ul>
          <div className="flex justify-end gap-2">
            <button type="button" className={catalogActionButton} data-initial-focus onClick={() => setDialog(null)}>{t('cancel')}</button>
            <button type="button" className={primaryButton} onClick={() => { setDialog(null); void setEnabled(true); }}>{t('tvEnableConfirm')}</button>
          </div>
        </CatalogActionDialog>
      )}
      {dialog === 'remove' && (
        <CatalogActionDialog title={t('tvRemoveTitle')} onClose={() => setDialog(null)}>
          <p>{t('tvRemoveText')}</p>
          <div className="flex justify-end gap-2">
            <button type="button" className={catalogActionButton} data-initial-focus onClick={() => setDialog(null)}>{t('cancel')}</button>
            <button type="button" className={`${catalogActionButton} !border-[var(--crit)] !text-[var(--crit)]`} onClick={() => void remove()}>{t('tvTokenRemove')}</button>
          </div>
        </CatalogActionDialog>
      )}
    </section>
  );
}
