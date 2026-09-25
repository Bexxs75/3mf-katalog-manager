import { useId, useMemo, useState, type KeyboardEvent as ReactKeyboardEvent } from 'react';
import { createPortal } from 'react-dom';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatVolumeMl } from '../i18n/format';
import type { FilamentSpool } from '../types';
import { useAnchoredPopup } from '../hooks/useAnchoredPopup';
import { restockSpoolLabel } from '../lib/filamentRestock';
import { parseConsumeAmount } from '../lib/resinConsume';
import { consumeResin } from '../lib/api/filament';

interface Props {
  spool: FilamentSpool;
  anchor: HTMLElement;
  onClose: () => void;
  onConsumed: (updated: FilamentSpool) => void;
}

const POPUP_MIN_WIDTH = 280;
const fieldClass =
  'w-full min-w-0 h-7 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel-2)] text-[var(--ink)] text-[12.5px] outline-0 focus:border-[var(--accent)]';
const buttonClass = 'h-[30px] px-3 rounded-md text-[12.5px] font-semibold cursor-pointer border';

/** "− Verbrauch" (v0.13.1, Mockup freigegeben): verbrauchte ml einer Resin-Flasche abbuchen. */
export function ConsumeResinPopover({ spool, anchor, onClose, onConsumed }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const id = useId();
  const [amount, setAmount] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const anchorRef = useMemo(() => ({ current: anchor }), [anchor]);
  const { popupRef, style } = useAnchoredPopup<HTMLElement, HTMLDivElement>(anchorRef, true, onClose, POPUP_MIN_WIDTH);

  const title = t('resinConsumeTitle').replace('{spool}', restockSpoolLabel(spool));

  const cancel = () => {
    onClose();
    anchor.focus();
  };

  const onKeyDown = (e: ReactKeyboardEvent) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      cancel();
    } else if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') {
      e.preventDefault();
      submit();
    }
  };

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
          {t('resinConsumeRemaining').replace('{amount}', formatVolumeMl(spool.remainingWeightG, language))}
        </p>
      </div>
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
          {t('resinConsumeSubmit')}
        </button>
      </div>
    </div>,
    document.body,
  );
}
