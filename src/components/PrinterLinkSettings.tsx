import { useT } from '../i18n/LanguageContext';
import type { Printer } from '../types';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { ToggleSwitch } from './ToggleSwitch';

interface Props {
  link: PrinterLinkState;
  printers: Printer[];
}

/** Inhalt des Einstellungs-Reiters „Drucker“. */
export function PrinterLinkSettings({ link, printers }: Props) {
  const t = useT();
  const statusOf = (printerId: string) => {
    const c = link.connections.find((x) => x.printerId === printerId);
    if (!c) return t('printerLinkStatusNotLinked');
    return c.lastError ? t('printerLinkStatusError') : t('printerLinkStatusConnected');
  };
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[length:var(--font-size-body)] font-semibold">{t('printerLinkTitle')}</div>
          <p className="mt-1 font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">{t('printerLinkDescription')}</p>
        </div>
        <ToggleSwitch checked={link.enabled} onChange={(on) => link.setEnabled(on)} label={t('printerLinkTitle')} />
      </div>
      {link.enabled && printers.length > 0 && (
        <ul className="flex flex-col gap-1.5 border-t border-[var(--line)] pt-2.5">
          {printers.map((p) => (
            <li key={p.id} className="flex justify-between gap-2 text-[12.5px]">
              <span className="truncate">{p.name}</span>
              <span className="font-mono-ui text-[11px] text-[var(--ink-3)] whitespace-nowrap">{statusOf(p.id)}</span>
            </li>
          ))}
        </ul>
      )}
      {link.enabled && (
        <p className="font-mono-ui text-[10.5px] leading-relaxed text-[var(--ink-3)]">{t('printerLinkManageHint')}</p>
      )}
    </div>
  );
}
