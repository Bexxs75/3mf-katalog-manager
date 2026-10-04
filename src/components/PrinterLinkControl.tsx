import { useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { toAppError, type AppError } from '../lib/errors';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import { ToggleSwitch } from './ToggleSwitch';
import { ErrorText } from '../diagnostics/ErrorText';

interface Props {
  link: PrinterLinkState;
}

/** Global printer connection switch in the master list footer. */
export function PrinterLinkControl({ link }: Props) {
  const t = useT();
  const [actionError, setActionError] = useState<AppError | null>(null);
  const [expanded, setExpanded] = useState(false);
  const toggle = (on: boolean) => {
    setActionError(null);
    Promise.resolve(link.setEnabled(on)).catch((e) => setActionError(toAppError(e)));
  };
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center gap-2.5 [&>button[aria-checked=true]]:bg-[var(--good)] [&>button>span]:bg-[var(--panel)] max-[1023px]:group-data-[collapsed=true]:justify-center">
        <ToggleSwitch checked={link.enabled} onChange={toggle} label={t('pmLink')} />
        <div className="max-[1023px]:group-data-[collapsed=true]:hidden">
          <b>{t('pmLink')}</b>
          <p className="text-[11.5px] text-[var(--ink-3)]">{t(link.enabled ? 'pmSyncInterval' : 'pmSwitchedOff')}</p>
        </div>
      </div>
      <div className="max-[1023px]:group-data-[collapsed=true]:hidden">
        <button type="button" className="text-[12px] text-[var(--ink-2)] underline underline-offset-2 focus-visible:outline-2"
          aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{t('pmWhatHappens')}</button>
        {expanded && <p className="mt-2 text-[12px] text-[var(--ink-2)]">{t('printerLinkDescription')}</p>}
      </div>
      {actionError && (
        <div role="alert" className="text-[12px] text-[var(--crit)]">
          {t('printerConnectionActionFailed').replace('{message}', '')}
          <ErrorText error={actionError} />
        </div>
      )}
      {link.error && (
        <div role="alert" className="text-[12px] text-[var(--crit)]">
          {t('printerConnectionActionFailed').replace('{message}', '')}
          <ErrorText error={link.error} />
        </div>
      )}
    </div>
  );
}
