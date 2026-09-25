import { useId, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatVolumeMl } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { restockSpoolLabel } from '../lib/filamentRestock';
import { parseConsumeAmount } from '../lib/resinConsume';
import { consumeResin } from '../lib/api/filament';
import { SpoolPopoverShell, fieldClass } from './SpoolPopoverShell';

interface Props {
  spool: FilamentSpool;
  anchor: HTMLElement;
  onClose: () => void;
  onConsumed: (updated: FilamentSpool) => void;
}

const POPUP_MIN_WIDTH = 280;

/**
 * "− Verbrauch" (v0.13.1, Mockup freigegeben): verbrauchte ml einer Resin-Flasche
 * abbuchen. Nutzt die gemeinsame Popover-Huelle SpoolPopoverShell (wie
 * RestockPopover); Enter im Mengenfeld loest zusaetzlich das Absenden aus.
 */
export function ConsumeResinPopover({ spool, anchor, onClose, onConsumed }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const id = useId();
  const [amount, setAmount] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const title = t('resinConsumeTitle').replace('{spool}', restockSpoolLabel(spool));

  const submit = () => {
    if (busy) return;
    const ml = parseConsumeAmount(amount);
    if (ml === null) {
      setError(t('stockInvalidAmount'));
      return;
    }
    setError(null);
    setBusy(true);
    consumeResin(spool.id, ml)
      .then((updated) => onConsumed(updated))
      .catch((e) => {
        setError(`${t('resinConsumeError')} (${String(e)})`);
        setBusy(false);
      });
  };

  const onKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') {
      e.preventDefault();
      submit();
    }
  };

  return (
    <SpoolPopoverShell
      anchor={anchor}
      onClose={onClose}
      minWidth={POPUP_MIN_WIDTH}
      title={title}
      subtitle={t('resinConsumeRemaining').replace('{amount}', formatVolumeMl(spool.remainingWeightG, language))}
      error={error}
      busy={busy}
      submitLabel={t('resinConsumeSubmit')}
      onSubmit={submit}
      onKeyDown={onKeyDown}
    >
      <div className="grid grid-cols-[auto_1fr] gap-x-2.5 items-center">
        <label htmlFor={`${id}-amount`} className="text-[12px] text-[var(--ink-2)]">
          {t('resinConsumeAmountLabel')}
        </label>
        <input
          id={`${id}-amount`}
          type="text"
          inputMode="decimal"
          autoFocus
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          className={fieldClass}
        />
      </div>
    </SpoolPopoverShell>
  );
}
