import { useCallback, useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';

export type SettingLoadState = 'loading' | 'ready' | 'failed';

/**
 * Loads one setting and tells loading, loaded and failed apart. A value of `null` is the loaded answer
 * "not supported here" (callers hide the block), so it is never confused with a failed read.
 * A later reload keeps the last confirmed value when it fails; every read reports its progress and failure.
 */
export function useLoadedSetting<T>(load: () => Promise<T>) {
  const [value, setValue] = useState<T | undefined>(undefined);
  const [state, setState] = useState<SettingLoadState>('loading');
  const [error, setError] = useState<AppError | null>(null);
  const mounted = useRef(true);
  const request = useRef(0);
  const loadRef = useRef(load);
  loadRef.current = load;

  /** Resolves with whether the read succeeded. */
  const reload = useCallback(async (): Promise<boolean> => {
    const mine = ++request.current;
    setState('loading');
    setError(null);
    try {
      const next = await loadRef.current();
      if (!mounted.current || mine !== request.current) return true;
      setValue(next);
      setError(null);
      setState('ready');
      return true;
    } catch (e) {
      if (mounted.current && mine === request.current) {
        setError(toAppError(e)); setState('failed');
      }
      return false;
    }
  }, []);

  useEffect(() => {
    // Set to true again on setup: StrictMode runs setup, cleanup, setup.
    mounted.current = true;
    void reload();
    return () => { mounted.current = false; };
  }, [reload]);

  /** Adopts a value the caller just wrote and the backend confirmed. */
  const confirm = useCallback((next: T) => { ++request.current; setValue(next); setError(null); setState('ready'); }, []);

  return { value, confirm, state, error, reload };
}

const retryButton = 'ml-1 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] focus-visible:outline focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/** Shown in place of a setting's control while it is read, or when reading it failed (with "Reload"). */
export function SettingLoadStatus({ state, label, error, onReload, className, keptValue = false }: {
  keptValue?: boolean;
  state: SettingLoadState;
  /** Name of the setting, so the message is not orphaned when the setting's own heading is not rendered. */
  label?: string;
  error?: AppError | null;
  onReload: () => void;
  className?: string;
}) {
  const t = useT();
  const { language } = useLanguage();
  const lastConfirmed = {
    de: 'Angezeigt wird der zuletzt bestätigte Wert.',
    en: 'Showing the last confirmed value.',
    es: 'Se muestra el último valor confirmado.',
    fr: 'La dernière valeur confirmée est affichée.',
  }[language];
  const retained = keptValue && <p className="text-caption text-[var(--ink-3)]">{lastConfirmed}</p>;
  if (state === 'ready') return null;
  const title = label && <p className="text-[length:var(--font-size-body)] font-semibold">{label}</p>;
  if (state === 'loading') {
    return (
      <div className={className}>
        {title}
        {retained}
        <p role="status" className="text-caption text-[var(--ink-3)]">{t('settingLoading')}</p>
      </div>
    );
  }
  return (
    <div className={`text-caption ${className ?? ''}`}>
      {title}
      {retained}
      <p role="alert" className="text-[var(--crit)] break-words">
        {t('settingLoadFailed')} <ErrorText error={error ?? null} />
      </p>
      <button type="button" className={retryButton} onClick={onReload}>{t('settingReload')}</button>
    </div>
  );
}

/**
 * Write feedback of a setting: progress while saving, or the failure.
 * `keptValue` is true when the screen still shows the last confirmed value; otherwise the person is asked to reload.
 */
export function SettingSaveStatus({ saving, error, keptValue, onReload, className }: {
  saving: boolean;
  error: AppError | null;
  keptValue: boolean;
  onReload?: () => void;
  className?: string;
}) {
  const t = useT();
  return (
    <>
      {saving && <p role="status" className={`text-caption text-[var(--ink-3)] ${className ?? ''}`}>{t('settingSaving')}</p>}
      {!saving && error && (
        <div className={`text-caption ${className ?? ''}`}>
          <p role="alert" className="text-[var(--crit)] break-words">
            {t(keptValue ? 'settingSaveFailedKept' : 'settingSaveFailedReload')} <ErrorText error={error} />
          </p>
          {!keptValue && onReload && <button type="button" className={retryButton} onClick={onReload}>{t('settingReload')}</button>}
        </div>
      )}
    </>
  );
}
