import { useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { messageOf } from '../lib/errors';
import type { Printer } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { ToggleSwitch } from './ToggleSwitch';

interface Props {
  link: PrinterLinkState;
  printers: Printer[];
}

/** Content of the "Printers" settings tab. */
export function PrinterLinkSettings({ link, printers }: Props) {
  const t = useT();
  const [actionError, setActionError] = useState<string | null>(null);
  // Resin printers have no printer connection.
  const linkable = printers.filter((p) => p.kind !== 'resin');
  const statusOf = (printerId: string): { text: string; dot: string } => {
    const c = link.connections.find((x) => x.printerId === printerId);
    if (!c) return { text: t('printerLinkStatusNotLinked'), dot: 'bg-[var(--ink-3)]' };
    // After a restore `paused` is set but `lastError` is empty;
    // without this branch the connection would wrongly look connected.
    if (c.paused && c.lastError !== 'auth_required') return { text: t('printerPausedRetest'), dot: 'bg-[var(--warn)]' };
    if (c.lastError) return { text: t('printerLinkStatusError'), dot: 'bg-[var(--crit)]' };
    return { text: t('printerLinkStatusConnected'), dot: 'bg-[var(--good)]' };
  };
  const toggle = (on: boolean) => {
    setActionError(null);
    Promise.resolve(link.setEnabled(on)).catch((e) => setActionError(messageOf(e)));
  };
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[length:var(--font-size-body)] font-semibold">{t('printerLinkTitle')}</div>
          <p className="mt-1 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">{t('printerLinkDescription')}</p>
        </div>
        <ToggleSwitch checked={link.enabled} onChange={toggle} label={t('printerLinkTitle')} />
      </div>
      {actionError && (
        <div role="alert" className="text-[12px] text-[var(--crit)]">
          {t('printerConnectionActionFailed').replace('{message}', () => actionError)}
        </div>
      )}
      {link.error && (
        <div role="alert" className="text-[12px] text-[var(--crit)]">
          {t('printerConnectionActionFailed').replace('{message}', () => link.error as string)}
        </div>
      )}
      {link.enabled && linkable.length > 0 && (
        <ul className="flex flex-col gap-1.5 border-t border-[var(--line)] pt-2.5">
          {linkable.map((p) => {
            const s = statusOf(p.id);
            return (
              <li key={p.id} className="flex items-center justify-between gap-2 text-[12.5px]">
                <span className="truncate">{p.name}</span>
                <span className="flex items-center gap-1.5 font-mono-ui text-[11px] text-[var(--ink-3)] whitespace-nowrap">
                  <span aria-hidden className={`w-1.5 h-1.5 rounded-full flex-none ${s.dot}`} />
                  {s.text}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {link.enabled && (
        <p className="font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">{t('printerLinkManageHint')}</p>
      )}
    </div>
  );
}
