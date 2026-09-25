import { useId, useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';
import { formatDecimalInput } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { useAnchoredPopup } from '../hooks/useAnchoredPopup';
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

interface Props {
  spool: FilamentSpool;
  /** Der "Nachkaufen"-Knopf, an dem das Fenster haengt. */
  anchor: HTMLElement;
  knownLocations: string[];
  onClose: () => void;
  onCreated: (created: FilamentSpool[]) => void;
}

const POPUP_MIN_WIDTH = 300;
const fieldClass =
  'w-full min-w-0 h-7 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel-2)] text-[var(--ink)] text-[12.5px] outline-0 focus:border-[var(--accent)]';
const labelClass = 'text-[12px] text-[var(--ink-2)]';
const stepButtonClass =
  'w-7 h-7 bg-[var(--panel-2)] text-[var(--ink)] text-[14px] cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed focus-visible:outline-2 focus-visible:outline-[var(--accent)]';
const buttonClass = 'h-[30px] px-3 rounded-md text-[12.5px] font-semibold cursor-pointer border';

/**
 * "Nachkaufen" (v0.13.1, Mockups freigegeben): legt N neue, volle Spulen bzw.
 * Resin-Flaschen nach dem Vorbild von `spool` an. Per Portal fixed
 * positioniert (useAnchoredPopup), damit der Scrollbereich des Lagers es nicht
 * abschneidet. Escape, "Abbrechen" und Klick daneben schliessen, ohne etwas
 * anzulegen.
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

  // Stabile Ref-Huelle, weil useAnchoredPopup eine RefObject erwartet.
  const anchorRef = useMemo(() => ({ current: anchor }), [anchor]);
  const { popupRef, style } = useAnchoredPopup<HTMLElement, HTMLDivElement>(anchorRef, true, onClose, POPUP_MIN_WIDTH);

  const title = t('filamentRestockTitle').replace('{spool}', restockSpoolLabel(spool));

  const cancel = () => {
    onClose();
    anchor.focus();
  };

  const onKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key !== 'Escape') return;
    e.preventDefault();
    e.stopPropagation();
    cancel();
  };

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

  if (!style) return null;

  return createPortal(
    <div
      ref={popupRef}
      role="dialog"
      aria-label={title}
      style={style}
      onKeyDown={onKeyDown}
      className="z-[60] rounded-[10px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] shadow-[var(--shadow)] p-3.5 flex flex-col gap-2.5"
    >
      <div>
        <h3 className="text-[13px] font-bold m-0">{title}</h3>
        <p className="text-[11.5px] text-[var(--ink-3)] m-0">
          {resin ? t('resinRestockSubtitle') : t('filamentRestockSubtitle')}
        </p>
      </div>

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

      {error && (
        <div role="alert" className="text-[11.5px] text-[var(--accent)] break-words">
          {error}
        </div>
      )}

      <div className="flex justify-end gap-1.5">
        <button type="button" onClick={cancel} className={`${buttonClass} border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)]`}>
          {t('cancel')}
        </button>
        <button
          type="button"
          onClick={submit}
          disabled={busy}
          className={`${buttonClass} border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          {formatCount(resin ? t('resinRestockSubmit') : t('filamentRestockSubmit'), count)}
        </button>
      </div>
    </div>,
    document.body,
  );
}
