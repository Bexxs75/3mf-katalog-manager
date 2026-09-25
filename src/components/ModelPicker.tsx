import { useEffect, useId, useMemo, useRef, useState, type RefObject } from 'react';
import { createPortal } from 'react-dom';
import { useAnchoredPopup } from '../hooks/useAnchoredPopup';
import { useT } from '../i18n/LanguageContext';

export interface ModelOption {
  id: string;
  name: string;
}

interface Props {
  models: ModelOption[];
  /** DOM-Knoten des Auswahl-Knopfs, an dem das Popup positioniert wird. */
  anchorRef: RefObject<HTMLElement | null>;
  onChange: (fileId: string | null) => void;
  onClose: () => void;
}

const NONE = Symbol('none');
type OptionId = string | typeof NONE;

// Entspricht der bisherigen festen Breite (w-[280px]) - useAnchoredPopup
// nimmt die groessere der beiden (Ankerbreite oder dieser Mindestbreite).
const POPUP_MIN_WIDTH = 280;

/**
 * Suchauswahl über alle Katalogmodelle, inkl. „Kein Modell“ - Pfeiltasten ab
 * dem Suchfeld und Enter waehlen aus, wie bei SpoolPicker. Wird per Portal
 * fixed relativ zum Auswahl-Knopf (`anchorRef`) gerendert, damit ein
 * ueberlaufender Dialog-Scrollcontainer (z.B. PrinterJobsDialog) das Popup
 * nicht abschneidet (siehe SpoolPicker, Commits 3365945 + 475c607).
 */
export function ModelPicker({ models, anchorRef, onChange, onClose }: Props) {
  const t = useT();
  const uid = useId();
  const [q, setQ] = useState('');
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const focusedRef = useRef(false);
  // `onClose` ist die einzige Weise, wie diese Komponente (von aussen
  // gesteuert) wieder verschwindet - solange sie gemountet ist, gilt sie als
  // offen.
  const { popupRef, style } = useAnchoredPopup<HTMLElement, HTMLDivElement>(anchorRef, true, onClose, POPUP_MIN_WIDTH);
  const hits = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (needle ? models.filter((m) => m.name.toLowerCase().includes(needle)) : models).slice(0, 50);
  }, [models, q]);
  // Index 0 ist immer "Kein Modell", danach die Treffer - eine gemeinsame
  // Liste fuer die Pfeiltasten-Navigation.
  const optionIds: OptionId[] = [NONE, ...hits.map((m) => m.id)];

  useEffect(() => setActive(0), [q]);

  // Suchfeld fokussieren, sobald das Popup tatsaechlich im DOM steht (nicht
  // schon beim ersten Rendern, da das Popup erst rendert, sobald
  // useAnchoredPopup eine Position berechnet hat). `preventScroll`, damit das
  // Fokussieren selbst in echten Browsern keinen Scroll ausloest - sonst
  // schliesst der eigene Scroll-Listener (in useAnchoredPopup) das Popup,
  // kaum dass es offen ist (dieselbe Regression, die bei SpoolPicker schon
  // einmal aufgetreten ist, siehe Commit 475c607).
  useEffect(() => {
    if (style && !focusedRef.current) {
      focusedRef.current = true;
      inputRef.current?.focus({ preventScroll: true });
    }
  }, [style]);

  const choose = (id: OptionId) => onChange(id === NONE ? null : id);
  const domId = (id: OptionId) => (id === NONE ? `${uid}-none` : `${uid}-${id}`);
  const activeId = optionIds[Math.min(active, optionIds.length - 1)];

  if (!style) return null;

  return createPortal(
    <div
      ref={popupRef}
      style={style}
      className="z-[60] rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] p-1.5 flex flex-col gap-1"
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          // Nicht bis zum umgebenden Dialog durchreichen - Escape soll hier
          // nur die Auswahl schliessen, nicht den ganzen Dialog.
          e.stopPropagation();
          onClose();
        } else if (e.key === 'ArrowDown') {
          e.preventDefault();
          setActive((a) => Math.min(optionIds.length - 1, a + 1));
        } else if (e.key === 'ArrowUp') {
          e.preventDefault();
          setActive((a) => Math.max(0, a - 1));
        } else if (e.key === 'Enter') {
          e.preventDefault();
          choose(activeId);
        }
      }}
    >
      <input
        ref={inputRef}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        placeholder={t('printerJobModelSearch')}
        role="combobox"
        aria-expanded
        aria-controls={`${uid}-listbox`}
        aria-activedescendant={domId(activeId)}
        className="h-7 px-2 rounded border border-[var(--line-strong)] bg-[var(--panel-2)] text-[12.5px] outline-0 focus:border-[var(--accent)]"
      />
      <ul id={`${uid}-listbox`} role="listbox" aria-label={t('printerJobsColModel')} className="max-h-52 overflow-y-auto flex flex-col gap-0.5">
        <li
          id={domId(NONE)}
          role="option"
          aria-selected={active === 0}
          onClick={() => choose(NONE)}
          className={`shrink-0 text-left px-2 py-1 text-[12.5px] text-[var(--ink-3)] rounded cursor-pointer ${active === 0 ? 'bg-[var(--panel-2)]' : ''}`}
        >
          {t('printerJobModelNoModel')}
        </li>
        {hits.map((m, i) => (
          <li
            key={m.id}
            id={domId(m.id)}
            role="option"
            aria-selected={active === i + 1}
            onClick={() => choose(m.id)}
            className={`shrink-0 px-2 py-1 text-[12.5px] rounded truncate cursor-pointer ${active === i + 1 ? 'bg-[var(--panel-2)]' : ''}`}
          >
            {m.name}
          </li>
        ))}
      </ul>
    </div>,
    document.body,
  );
}
