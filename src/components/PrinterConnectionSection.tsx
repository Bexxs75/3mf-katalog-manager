import { useEffect, useRef, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatDateTime } from '../i18n/format';
import { messageOf } from '../lib/errors';
import type { PrinterConnection, PrinterConnectionError } from '../types';
import { formatCount, type PluralForms } from '../i18n/types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';

interface Props {
  printerId: string;
  connection: PrinterConnection | null;
  link: PrinterLinkState;
}

export type PrinterErrorKey =
  | 'printerErrorUnreachable'
  | 'printerErrorAuthRequired'
  | 'printerErrorBadResponse'
  | 'printerErrorHistoryMissing'
  | 'printerErrorAddressNotAllowed'
  | 'printerErrorDisabled';

/** Also used by `PrinterLinkStatus` so error texts aren't maintained twice. */
export const errorKey: Record<PrinterConnectionError, PrinterErrorKey> = {
  unreachable: 'printerErrorUnreachable',
  auth_required: 'printerErrorAuthRequired',
  bad_response: 'printerErrorBadResponse',
  history_missing: 'printerErrorHistoryMissing',
  address_not_allowed: 'printerErrorAddressNotAllowed',
  disabled: 'printerErrorDisabled',
};

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
  'h-8 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] outline-0 text-[12.5px] font-mono-ui focus:border-[var(--accent)] min-w-0';
const smallButton =
  'h-7 px-2 rounded-[4px] border border-[var(--line-strong)] text-[11.5px] text-[var(--ink-2)] hover:border-[var(--accent)] cursor-pointer disabled:opacity-50';

/** Connection of a printer (type, address, test). Only visible with the printer connection enabled. */
export function PrinterConnectionSection({ printerId, connection, link }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [address, setAddress] = useState(connection?.address ?? '');
  const [testing, setTesting] = useState(false);
  const [removing, setRemoving] = useState(false);
  const [error, setError] = useState<PrinterConnectionError | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
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
    setTesting(true);
    setError(null);
    setActionError(null);
    try {
      const r = await link.testConnection(printerId, address.trim());
      if (r.ok && r.connection) {
        skipNextPropSync.current = true;
        setCurrent(r.connection);
      } else {
        setError(r.error ?? 'bad_response');
      }
    } catch (e) {
      setActionError(messageOf(e));
    } finally {
      setTesting(false);
    }
  };

  const runRemove = async () => {
    setRemoving(true);
    setActionError(null);
    try {
      await link.removeConnection(printerId);
      setCurrent(null);
    } catch (e) {
      setActionError(messageOf(e));
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

  return (
    <div className="mt-3 rounded-md border border-[var(--line)] bg-[var(--panel)] p-3 flex flex-col gap-2.5">
      <div className="font-mono-ui text-[10.5px] uppercase tracking-[0.12em] text-[var(--ink-3)]">{t('printerConnectionTitle')}</div>
      <div className="grid grid-cols-1 sm:grid-cols-[170px_minmax(0,1fr)] gap-2">
        <div className="flex flex-col gap-1 min-w-0">
          <span className="text-[11.5px] text-[var(--ink-2)]">{t('printerConnectionType')}</span>
          <div className={`${fieldClass} flex items-center font-sans min-w-0`}>{t('printerConnectionTypeMoonraker')}</div>
        </div>
        <label className="flex flex-col gap-1 min-w-0">
          <span className="text-[11.5px] text-[var(--ink-2)]">{t('printerConnectionAddress')}</span>
          <input
            className={`${fieldClass} w-full min-w-0 placeholder:text-[var(--ink-3)]`}
            value={address}
            onChange={(e) => setAddress(e.target.value)}
            placeholder="192.168.1.60"
            spellCheck={false}
            autoComplete="off"
          />
        </label>
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <button type="button" className={smallButton} disabled={testing || address.trim() === ''} onClick={runTest}>
          {testing ? t('printerConnectionTesting') : t('printerConnectionTest')}
        </button>
        {current && (
          <button type="button" className={smallButton} disabled={removing} onClick={runRemove}>
            {t('printerConnectionRemove')}
          </button>
        )}
      </div>
      {actionError && (
        <div role="alert" className="border-l-[3px] border-[var(--crit)] pl-2.5 text-[12px] text-[var(--ink-2)]">
          {t('printerConnectionActionFailed').replace('{message}', actionError)}
        </div>
      )}
      {error && (
        <div role="alert" className="border-l-[3px] border-[var(--crit)] pl-2.5 text-[12px] text-[var(--ink-2)]">
          {t(errorKey[error])}
        </div>
      )}
      {!error && current && !current.paused && !current.lastError && (
        <div className="border-l-[3px] border-[var(--good)] pl-2.5 flex flex-col gap-0.5">
          <b className="text-[12.5px]">{t('printerConnectionOk')}</b>
          <span className="font-mono-ui text-[11px] text-[var(--ink-2)]">
            {t('printerConnectionOkDetail')
              .replace('{version}', current.remoteVersion ?? '?')
              .replace('{port}', t('printerConnectionPort').replace('{port}', portOf(current.baseUrl)))}
          </span>
          <span className="text-[11.5px] text-[var(--ink-2)]">{t('printerConnectionSince').replace('{date}', since)}</span>
        </div>
      )}
      {!error && current && !current.paused && !current.lastError && Math.abs(clockOffset) >= CLOCK_NOTE_MIN_S && (
        <div role="status" className="border-l-[3px] border-[var(--warn)] pl-2.5 flex flex-col gap-0.5">
          <b className="text-[12.5px]">{t('printerClockOffTitle')}</b>
          <span className="font-mono-ui text-[11px] text-[var(--ink)]">
            {t('printerClockOffTimes')
              .replace('{printer}', () => formatDateTime(localNow + clockOffset, language))
              .replace('{local}', () => formatDateTime(localNow, language))}
          </span>
          <span className="text-[11.5px] text-[var(--ink-2)]">{t('printerClockOffBody').replace('{amount}', () => clockAmount)}</span>
        </div>
      )}
    </div>
  );
}
