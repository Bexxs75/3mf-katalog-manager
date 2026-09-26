import { useId, useMemo, useState } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';
import { formatDecimalInput } from '../i18n/format';
import type { FilamentSpool } from '../types';
import {
  RESTOCK_MAX_COUNT,
  RESTOCK_MIN_COUNT,
  clampRestockCount,
  restockDefaults,
  restockSpoolLabel,
  validateRestockInput,
} from '../lib/filamentRestock';
import { restockFilamentSpool } from '../lib/api/filament';
import { AutocompleteInput } from './AutocompleteInput';
import { SpoolPopoverShell, fieldClass } from './SpoolPopoverShell';

interface Props {
  spool: FilamentSpool;
  /** The "Restock" button the popover hangs on. */
  anchor: HTMLElement;
  knownLocations: string[];
  onClose: () => void;
  onCreated: (created: FilamentSpool[]) => void;
}

const POPUP_MIN_WIDTH = 300;
const labelClass = 'text-[12px] text-[var(--ink-2)]';
const stepButtonClass =
  'w-7 h-7 bg-[var(--panel-2)] text-[var(--ink)] text-[14px] cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed focus-visible:outline-2 focus-visible:outline-[var(--accent)]';

/**
 * "Restock": creates N new, full spools or resin bottles modeled on
 * `spool` (shell: SpoolPopoverShell).
 */
export function RestockPopover({ spool, anchor, knownLocations, onClose, onCreated }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const id = useId();
  const resin = spool.kind === 'resin';
  const defaults = useMemo(() => restockDefaults(spool), [spool]);
  const [count, setCount] = useState(RESTOCK_MIN_COUNT);
  const [weight, setWeight] = useState(() => formatDecimalInput(defaults.weight, language));
  const [price, setPrice] = useState(() => (defaults.price === null ? '' : formatDecimalInput(defaults.price, language)));
  const [location, setLocation] = useState(defaults.location);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const title = t('filamentRestockTitle').replace('{spool}', restockSpoolLabel(spool));

  const submit = () => {
    if (busy) return;
    const result = validateRestockInput({ count, weight, price, location });
    if (!result.ok) {
      setError(result.error === 'weight' ? t('stockInvalidAmount') : t('filamentRestockInvalidPrice'));
      return;
    }
    setError(null);
    setBusy(true);
    const { value } = result;
    restockFilamentSpool(spool.id, value.count, value.weight, value.price, value.location)
      .then((created) => onCreated(created))
      .catch((e) => {
        setError(`${t('filamentRestockError')} (${String(e)})`);
        setBusy(false);
      });
  };

  return (
    <SpoolPopoverShell
      anchor={anchor}
      onClose={onClose}
      minWidth={POPUP_MIN_WIDTH}
      title={title}
      subtitle={resin ? t('resinRestockSubtitle') : t('filamentRestockSubtitle')}
      error={error}
      busy={busy}
      submitLabel={formatCount(resin ? t('resinRestockSubmit') : t('filamentRestockSubmit'), count)}
      onSubmit={submit}
    >
      <div className="grid grid-cols-[auto_1fr] gap-x-2.5 gap-y-2 items-center">
        <span className={labelClass}>{t('filamentRestockCountLabel')}</span>
        <span className="inline-flex items-center w-max rounded-md border border-[var(--line-strong)] overflow-hidden">
          <button
            type="button"
            aria-label={t('filamentRestockDecrease')}
            disabled={count <= RESTOCK_MIN_COUNT}
            onClick={() => setCount((c) => clampRestockCount(c - 1))}
            className={stepButtonClass}
          >
            −
          </button>
          <output aria-live="polite" className="w-9 text-center font-mono-ui text-[13px] font-semibold">
            {count}
          </output>
          <button
            type="button"
            autoFocus
            aria-label={t('filamentRestockIncrease')}
            disabled={count >= RESTOCK_MAX_COUNT}
            onClick={() => setCount((c) => clampRestockCount(c + 1))}
            className={stepButtonClass}
          >
            +
          </button>
        </span>

        <label htmlFor={`${id}-weight`} className={labelClass}>
          {resin ? t('resinAmountLabel') : t('filamentRestockWeightLabel')}
        </label>
        <input
          id={`${id}-weight`}
          type="text"
          inputMode="decimal"
          value={weight}
          onChange={(e) => setWeight(e.target.value)}
          className={fieldClass}
        />

        <label htmlFor={`${id}-price`} className={labelClass}>
          {resin ? t('resinRestockPriceLabel') : t('filamentRestockPriceLabel')}
        </label>
        <input
          id={`${id}-price`}
          type="text"
          inputMode="decimal"
          value={price}
          onChange={(e) => setPrice(e.target.value)}
          className={fieldClass}
        />

        <span className={labelClass}>{t('filamentLocationLabel')}</span>
        <AutocompleteInput
          value={location}
          onChange={setLocation}
          options={knownLocations}
          placeholder={t('filamentLocationLabel')}
          className={fieldClass}
        />
      </div>
    </SpoolPopoverShell>
  );
}
