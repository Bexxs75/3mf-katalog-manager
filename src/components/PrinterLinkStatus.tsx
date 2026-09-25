import { useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatRelativeTime } from '../i18n/format';
import type { PrinterLinkState } from '../hooks/usePrinterLink';

interface Props {
  printerId: string;
  link: PrinterLinkState;
}

const iso = (unixSeconds: number) => new Date(unixSeconds * 1000).toISOString();

function messageOf(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Status-Zeile eines angebundenen Druckers in der Druckerspalte. */
export function PrinterLinkStatus({ printerId, link }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [syncError, setSyncError] = useState<string | null>(null);
  if (!link.enabled) return null;
  const c = link.connections.find((x) => x.printerId === printerId);
  if (!c) return null;
  const pending = link.jobs.filter((j) => j.printerId === printerId).length;

  let dot = 'bg-[var(--good)]';
  let text = c.lastSyncedAt ? t('printerSyncedAgo').replace('{time}', formatRelativeTime(iso(c.lastSyncedAt), language)) : t('printerNotSyncedYet');
  if (c.lastError === 'auth_required') {
    dot = 'bg-[var(--warn)]';
    text = t('printerAuthNeeded');
  } else if (c.lastError) {
    dot = 'bg-[var(--crit)]';
    const time = new Intl.DateTimeFormat(language, { timeStyle: 'short' }).format(new Date((c.errorSince ?? 0) * 1000));
    text = t('printerUnreachableSince').replace('{time}', time);
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-center gap-2 text-[11.5px] text-[var(--ink-2)]">
        <span className={`w-2 h-2 rounded-full flex-none ${dot}`} />
        <span className="truncate">{text}</span>
        <button
          type="button"
          aria-label={t('printerSyncNow')}
          title={t('printerSyncNow')}
          onClick={() => {
            setSyncError(null);
            Promise.resolve(link.syncNow()).catch((e) => setSyncError(messageOf(e)));
          }}
          className="ml-auto h-5 px-1.5 rounded border border-[var(--line)] text-[var(--ink-2)] hover:border-[var(--accent)] cursor-pointer"
        >
          ↻
        </button>
      </div>
      {pending > 0 && (
        <div className="text-[11.5px] text-[var(--accent)]">
          <span aria-hidden>●</span> {t('printerJobsPending').replace('{count}', String(pending))}
        </div>
      )}
      {syncError && (
        <div className="text-[11px] text-[var(--crit)]">
          {t('printerConnectionActionFailed').replace('{message}', syncError)}
        </div>
      )}
    </div>
  );
}
