import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { formatDateTime } from '../i18n/format';
import { toAppError, type AppError } from '../lib/errors';
import type { PrinterConnection, PrinterConnectionError } from '../types';
import type { PluralForms } from '../i18n/types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { ErrorText } from '../diagnostics/ErrorText';

interface Props {
  printerId: string;
  connection: PrinterConnection | null;
  link: PrinterLinkState;
}

export type PrinterErrorKey =
  | 'printerErrorNameResolutionFailed'
  | 'printerErrorContainerLocalName'
  | 'printerErrorContainerLoopback'
  | 'printerErrorUnreachable'
  | 'printerErrorTimeout'
  | 'printerErrorConnectionRefused'
  | 'printerErrorConnectionReset'
  | 'printerErrorServerBusy'
  | 'printerErrorAuthRequired'
  | 'printerErrorAuthRejected'
  | 'printerErrorApiKeyInvalid'
  | 'printerErrorBadResponse'
  | 'printerErrorTooManyOpenJobs'
  | 'printerErrorHistoryMissing'
  | 'printerErrorAddressNotAllowed'
  | 'printerErrorDisabled';

/** Also used by `PrinterLinkStatus` so error texts aren't maintained twice. */
export const errorKey: Record<PrinterConnectionError, PrinterErrorKey> = {
  name_resolution_failed: 'printerErrorNameResolutionFailed',
  container_local_name: 'printerErrorContainerLocalName',
  container_loopback: 'printerErrorContainerLoopback',
  unreachable: 'printerErrorUnreachable',
  timeout: 'printerErrorTimeout',
  connection_refused: 'printerErrorConnectionRefused',
  connection_reset: 'printerErrorConnectionReset',
  server_busy: 'printerErrorServerBusy',
  auth_required: 'printerErrorAuthRequired',
  auth_rejected: 'printerErrorAuthRejected',
  api_key_invalid: 'printerErrorApiKeyInvalid',
  bad_response: 'printerErrorBadResponse',
  too_many_open_jobs: 'printerErrorTooManyOpenJobs',
  history_missing: 'printerErrorHistoryMissing',
  address_not_allowed: 'printerErrorAddressNotAllowed',
  disabled: 'printerErrorDisabled',
};

/** Never returns an empty text, even for a code this version does not know. */
export function errorKeyFor(code: string): PrinterErrorKey {
  return (errorKey as Record<string, PrinterErrorKey>)[code] ?? 'printerErrorUnknown';
}

export function printerErrorHost(address: string): string {
  return address.trim().replace(/:\d+$/, '');
}

function portOf(baseUrl: string | null): string {
  if (!baseUrl) return '';
  const m = baseUrl.match(/:(\d+)$/);
  return m ? m[1] : '80';
}

/** Offsets below this are ignored (the backend already stores 0 then). */
export const CLOCK_NOTE_MIN_S = 300;

export type ClockUnit = 'minutes' | 'hours' | 'days' | 'months' | 'years';

/** Rough size of a clock offset: minutes, hours, days, or years plus months. Months are cut, not rounded. */
export function clockOffsetParts(offsetS: number): Array<[ClockUnit, number]> {
  const s = Math.abs(offsetS);
  if (s < 3600) return [['minutes', Math.round(s / 60)]];
  if (s < 2 * 86400) return [['hours', Math.round(s / 3600)]];
  if (s < 60 * 86400) return [['days', Math.round(s / 86400)]];
  const months = Math.floor(s / (30.4375 * 86400));
  const parts: Array<[ClockUnit, number]> = [];
  if (months >= 12) parts.push(['years', Math.floor(months / 12)]);
  if (months % 12 > 0) parts.push(['months', months % 12]);
  return parts;
}

const fieldClass =
  'h-8 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] outline-none text-small focus:border-[var(--accent)] min-w-0';
const smallButton =
  'max-[639px]:w-full h-8 px-3 rounded-[4px] border border-[var(--line-strong)] text-caption text-[var(--ink-2)] hover:border-[var(--accent)] cursor-pointer disabled:opacity-50';

/** Connection of a printer (type, address, test). The global switch controls testing and sync. */
export function PrinterConnectionSection({ printerId, connection, link }: Props) {
  const t = useT();
  const formatCount = useFormatCount();
  const { language } = useLanguage();
  const [address, setAddress] = useState(connection?.address ?? '');
  const [testedAddress, setTestedAddress] = useState('');
  const [testing, setTesting] = useState(false);
  const [removing, setRemoving] = useState(false);
  // What the user types. Never filled from the backend: it only says that a key exists.
  const [apiKey, setApiKey] = useState('');
  const [showKey, setShowKey] = useState(false);
  const [replacingKey, setReplacingKey] = useState(false);
  const [clearingKey, setClearingKey] = useState(false);
  const [readingOlder, setReadingOlder] = useState(false);
  // What the last "include older history" found: a count, or the error code of the connection.
  const [olderResult, setOlderResult] = useState<{ added: number } | { error: string } | null>(null);
  const [error, setError] = useState<PrinterConnectionError | null>(null);
  const [actionError, setActionError] = useState<AppError | null>(null);
  const [current, setCurrent] = useState<PrinterConnection | null>(connection);
  // Prevents the prop sync below from overwriting a result just delivered by
  // `runTest` with a still stale `connection` prop.
  const skipNextPropSync = useRef(false);

  // Take over later prop changes (e.g. from the background sync).
  useEffect(() => {
    if (skipNextPropSync.current) {
      skipNextPropSync.current = false;
      return;
    }
    setCurrent(connection);
  }, [connection]);

  const runTest = async () => {
    setTestedAddress(address.trim());
    setTesting(true);
    setError(null);
    setActionError(null);
    try {
      const r = await link.testConnection(printerId, address.trim(), apiKey.trim() === '' ? null : apiKey.trim());
      if (r.ok && r.connection) {
        skipNextPropSync.current = true;
        setCurrent(r.connection);
        // The key is saved; the field must not keep it.
        setApiKey('');
        setShowKey(false);
        setReplacingKey(false);
      } else {
        setError(r.error ?? 'bad_response');
      }
    } catch (e) {
      setActionError(toAppError(e));
    } finally {
      setTesting(false);
    }
  };

  const runClearKey = async () => {
    setClearingKey(true);
    setActionError(null);
    try {
      await link.clearApiKey(printerId);
      setCurrent((c) => (c ? { ...c, hasApiKey: false } : c));
      setReplacingKey(false);
      setApiKey('');
    } catch (e) {
      setActionError(toAppError(e));
    } finally {
      setClearingKey(false);
    }
  };

  const runOlderHistory = async () => {
    setReadingOlder(true);
    setOlderResult(null);
    setActionError(null);
    try {
      const r = await link.includeOlderHistory(printerId);
      setOlderResult(r.error ? { error: r.error } : { added: r.added });
    } catch (e) {
      setActionError(toAppError(e));
    } finally {
      setReadingOlder(false);
    }
  };

  const runRemove = async () => {
    setRemoving(true);
    setActionError(null);
    try {
      await link.removeConnection(printerId);
      setCurrent(null);
      setApiKey('');
      setReplacingKey(false);
    } catch (e) {
      setActionError(toAppError(e));
    } finally {
      setRemoving(false);
    }
  };

  const since = current ? formatDateTime(current.connectedSince, language) : '';
  const unitForms: Record<ClockUnit, PluralForms> = {
    minutes: t('printerClockMinutes'),
    hours: t('printerClockHours'),
    days: t('printerClockDays'),
    months: t('printerClockMonths'),
    years: t('printerClockYears'),
  };
  const clockOffset = current?.clockOffsetS ?? 0;
  const clockAmount = clockOffsetParts(clockOffset)
    .map(([unit, n]) => formatCount(unitForms[unit], n))
    .reduce((a, b) => t('printerClockAnd').replace('{a}', () => a).replace('{b}', () => b));
  const localNow = Date.now() / 1000;
  const effectiveError = error ?? current?.lastError;
  // A printer that is only switched off or restarting is not an alarm: a failed test the user
  // started is, and so is anything that needs them.
  const soft = !error && (current?.status === 'reconnecting' || current?.status === 'offline') ? current.status : null;

  if (import.meta.env.VITE_TARGET === 'web') return null;
  return (
    <div className="flex flex-col gap-3">
      <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
        <div className="flex flex-col gap-1 min-w-0">
          <span className="text-caption text-[var(--ink-2)]">{t('pmConnectionType')}</span>
          <div className={`${fieldClass} flex items-center font-sans min-w-0`}>{t('printerConnectionTypeMoonraker')}</div>
        </div>
        <label className="flex flex-col gap-1 min-w-0">
          <span className="text-caption text-[var(--ink-2)]">{t('pmHost')}</span>
          <input
            className={`${fieldClass} font-code w-full min-w-0 placeholder:text-[var(--ink-3)]`}
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="192.168.1.60"
            spellCheck={false}
            autoComplete="off"
          />
        </label>
        {current?.hasApiKey && !replacingKey ? (
          <div className="flex flex-col gap-1 min-w-0">
            <span className="text-caption text-[var(--ink-2)]">{t('pmApiKey')}</span>
            <div className="flex flex-wrap items-center gap-2 min-h-8">
              <span className="text-small text-[var(--ink)]">{t('pmApiKeySaved')}</span>
              <button type="button" className={smallButton} onClick={() => setReplacingKey(true)}>{t('pmApiKeyReplace')}</button>
              <button type="button" className={smallButton} disabled={clearingKey} onClick={runClearKey}>{t('pmApiKeyRemove')}</button>
            </div>
          </div>
        ) : (
          <div className="flex flex-col gap-1 min-w-0">
            <label htmlFor={`printer-api-key-${printerId}`} className="text-caption text-[var(--ink-2)]">{t('pmApiKey')}</label>
            <div className="flex items-center gap-2 min-w-0">
              <input
                id={`printer-api-key-${printerId}`}
                className={`${fieldClass} font-code w-full min-w-0 placeholder:text-[var(--ink-3)] disabled:bg-[var(--panel-2)]`}
                type={showKey ? 'text' : 'password'}
                disabled={!link.enabled}
                value={apiKey}
                onChange={(e) => setApiKey(e.target.value)}
                placeholder={t('pmApiPlaceholder')}
                spellCheck={false}
                autoComplete="off"
                autoCapitalize="off"
                autoCorrect="off"
              />
              <button type="button" className={smallButton} aria-pressed={showKey} onClick={() => setShowKey((v) => !v)}>
                {t(showKey ? 'pmApiKeyHide' : 'pmApiKeyShow')}
              </button>
            </div>
          </div>
        )}
      </div>
      <p className="text-caption text-[var(--ink-3)]">{t('pmApiKeyHint')}</p>
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={smallButton} disabled={!link.enabled || testing || address.trim() === ''} onClick={runTest}>
          {testing ? t('printerConnectionTesting') : t(current ? 'printerConnectionTest' : 'pmSetupConnection')}
        </button>
        {current && (
          <button type="button" className={smallButton} disabled={!link.enabled || readingOlder || removing} onClick={runOlderHistory}>
            {readingOlder ? t('printerOlderHistoryReading') : t('printerOlderHistory')}
          </button>
        )}
        {current && (
          <button type="button" className={smallButton} disabled={removing} onClick={runRemove}>
            {t('printerConnectionRemove')}
          </button>
        )}
      </div>
      {current && <p className="text-caption text-[var(--ink-3)]">{t('printerOlderHistoryHint')}</p>}
      {olderResult && 'error' in olderResult && (
        <div role="alert" className="rounded-lg bg-[var(--crit-soft)] p-3 text-small text-[var(--crit)]">
          <p>{t(errorKeyFor(olderResult.error)).replace('{host}', () => printerErrorHost(current?.address ?? ''))}</p>
        </div>
      )}
      {olderResult && 'added' in olderResult && (
        <div role="status" className="text-small text-[var(--ink-2)]">
          {olderResult.added > 0 ? formatCount(t('printerOlderHistoryAdded'), olderResult.added) : t('printerOlderHistoryNone')}
        </div>
      )}
      {!link.enabled ? <div className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] p-3"><b>{t('pmSwitchedOff')}</b><p>{t('pmLinkOff')}</p></div> :
        !current && !error && <div className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] p-3"><b>{t('pmNotLinked')}</b></div>}
      {actionError && (
        <div role="alert" className="rounded-lg bg-[var(--crit-soft)] p-3 text-small text-[var(--crit)]">
          {t('printerConnectionActionFailed').replace('{message}', '')}
          <ErrorText error={actionError} />
        </div>
      )}
      {link.enabled && soft && current && (
        <div role="status" className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] p-3 text-small text-[var(--ink-2)]">
          <b>{t(soft === 'reconnecting' ? 'pmReconnecting' : 'pmOffline')}</b>
          <p>
            {soft === 'reconnecting'
              ? t('printerReconnecting').replace('{n}', String((current.failures ?? 0) + 1))
              : t('printerUnreachableSince').replace('{time}', () => formatDateTime(current.errorSince ?? 0, language))}
          </p>
          {effectiveError && <p>{t(errorKeyFor(effectiveError)).replace('{host}', () => printerErrorHost(current.address))}</p>}
        </div>
      )}
      {link.enabled && !soft && (error || current?.lastError || current?.paused) && (
        <div role="alert" className="rounded-lg bg-[var(--crit-soft)] p-3 text-small text-[var(--crit)]">
          <b>{t('pmConnectionError')}</b><p>{effectiveError ? t(errorKeyFor(effectiveError)).replace('{host}', () => printerErrorHost(error ? testedAddress : current?.address ?? '')) : t('printerPausedRetest')}</p>
        </div>
      )}
      {link.enabled && !error && current && !current.paused && !current.lastError && (
        <div className="rounded-lg bg-[var(--good-soft)] p-3 text-[var(--good)] flex flex-col gap-0.5">
          <b className="text-small">{t('printerConnectionOk')}</b>
          <span className="font-medium tabular-nums text-caption text-[var(--ink-2)]">
            {t('printerConnectionOkDetail')
              .replace('{version}', current.remoteVersion ?? '?')
              .replace('{port}', t('printerConnectionPort').replace('{port}', portOf(current.baseUrl)))}
          </span>
          <span className="text-caption text-[var(--ink-2)]">{t('printerConnectionSince').replace('{date}', since)}</span>
        </div>
      )}
      {link.enabled && !error && current && !current.paused && !current.lastError && Math.abs(clockOffset) >= CLOCK_NOTE_MIN_S && (
        <div role="status" className="border-l-[3px] border-[var(--warn)] pl-2.5 flex flex-col gap-0.5">
          <b className="text-small">{t('printerClockOffTitle')}</b>
          <span className="font-medium tabular-nums text-caption text-[var(--ink)]">
            {t('printerClockOffTimes')
              .replace('{printer}', () => formatDateTime(localNow + clockOffset, language))
              .replace('{local}', () => formatDateTime(localNow, language))}
          </span>
          <span className="text-caption text-[var(--ink-2)]">{t('printerClockOffBody').replace('{amount}', () => clockAmount)}</span>
        </div>
      )}

    </div>
  );
}
