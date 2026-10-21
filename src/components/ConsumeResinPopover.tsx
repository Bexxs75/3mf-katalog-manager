import { useId, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatVolumeMl } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { restockSpoolLabel } from '../lib/filamentRestock';
import { suggestSpool } from '../lib/spoolSuggestion';
import { parseConsumeAmount } from '../lib/resinConsume';
import { consumeResin } from '../lib/api/filament';
import { expectedError, messageOf, toAppError, type AppError } from '../lib/errors';
import { SpoolPopoverShell, fieldClass } from './SpoolPopoverShell';

interface Props {
  spool: FilamentSpool;
  /**
   * Equal bottles to choose from (a group of the inventory). Then the one with the smallest
   * rest that still covers the entered amount is preselected, until the user picks one.
   */
  choices?: FilamentSpool[];
  anchor: HTMLElement;
  onClose: () => void;
  onConsumed: (updated: FilamentSpool) => void;
}

const POPUP_MIN_WIDTH = 280;

/**
 * "− Usage": deducts used ml from a resin bottle (shell:
 * SpoolPopoverShell); Enter in the amount field submits.
 */
export function ConsumeResinPopover({ spool: initial, choices, anchor, onClose, onConsumed }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const id = useId();
  const [amount, setAmount] = useState('');
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [pickedId, setPickedId] = useState<string | null>(null);
  const amountMl = parseConsumeAmount(amount);
  const suggested = choices ? suggestSpool(choices, amountMl ?? 0) : null;
  const spool = (choices ?? []).find((s) => s.id === (pickedId ?? suggested?.id ?? initial.id)) ?? initial;

  const title = t('resinConsumeTitle').replace('{spool}', restockSpoolLabel(spool));

  const submit = () => {
    if (busy) return;
    const ml = amountMl;
    if (ml === null) {
      setError(expectedError(t('stockInvalidAmount')));
      return;
    }
    setError(null);
    setBusy(true);
    consumeResin(spool.id, ml)
      .then((updated) => onConsumed(updated))
      .catch((e) => {
        setError({ ...toAppError(e), message: `${t('resinConsumeError')} (${messageOf(e)})` });
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
      <div className="grid grid-cols-[auto_1fr] gap-x-2.5 gap-y-2 items-center">
        {choices && choices.length > 1 && (
          <>
            <label htmlFor={`${id}-bottle`} className="text-small text-[var(--ink-2)]">
              {t('resinConsumeBottleLabel')}
            </label>
            <select
              id={`${id}-bottle`}
              value={spool.id}
              onChange={(e) => setPickedId(e.target.value)}
              className={fieldClass}
            >
              {choices.map((bottle) => (
                <option key={bottle.id} value={bottle.id}>
                  {[bottle.location ?? bottle.homeLocation, formatVolumeMl(bottle.remainingWeightG, language)].filter(Boolean).join(' · ')}
                  {bottle.id === suggested?.id && pickedId === null ? ` (${t('resinSuggestedBottle')})` : ''}
                </option>
              ))}
            </select>
          </>
        )}
        <label htmlFor={`${id}-amount`} className="text-small text-[var(--ink-2)]">
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
