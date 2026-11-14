import { useT } from '../i18n/LanguageContext';
import type { Printer, PrinterConnection } from '../types';

export function printerStatus(printer: Printer, enabled: boolean, connection?: PrinterConnection) {
  if (printer.kind === 'resin' || !enabled || !connection) return 'pmInactive';
  switch (connection.status) {
    case 'ok': return 'pmConnected';
    case 'reconnecting': return 'pmReconnecting';
    case 'offline': return 'pmOffline';
    case 'error':
    case 'paused': return 'pmConnectionError';
    // Data without a derived status: the old rule.
    default: return connection.lastError || connection.paused ? 'pmConnectionError' : 'pmConnected';
  }
}

export const printerStatusColors = {
  pmInactive: 'text-[var(--unk)] bg-[var(--unk-soft)]',
  pmConnectionError: 'text-[var(--crit)] bg-[var(--crit-soft)]',
  pmReconnecting: 'text-[var(--ink-2)] bg-[var(--panel-2)]',
  pmOffline: 'text-[var(--unk)] bg-[var(--unk-soft)]',
  pmConnected: 'text-[var(--good)] bg-[var(--good-soft)]',
};

export function PrinterStatusBadge({ status }: { status: keyof typeof printerStatusColors }) {
  const t = useT();
  return <span className={`inline-flex shrink-0 items-center gap-1.5 rounded-full px-2 py-1 text-caption font-semibold ${printerStatusColors[status]}`}>
    <span aria-hidden="true" className="h-1.5 w-1.5 rounded-full bg-current" />{t(status)}
  </span>;
}
