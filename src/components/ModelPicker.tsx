import { useEffect, useId, useMemo, useState } from 'react';
import { useT } from '../i18n/LanguageContext';

export interface ModelOption {
  id: string;
  name: string;
}

interface Props {
  models: ModelOption[];
  onChange: (fileId: string | null) => void;
  onClose: () => void;
}

const NONE = Symbol('none');
type OptionId = string | typeof NONE;

/**
 * Suchauswahl über alle Katalogmodelle, inkl. „Kein Modell“ - Pfeiltasten ab
 * dem Suchfeld und Enter waehlen aus, wie bei SpoolPicker.
 */
export function ModelPicker({ models, onChange, onClose }: Props) {
  const t = useT();
  const uid = useId();
  const [q, setQ] = useState('');
  const [active, setActive] = useState(0);
  const hits = useMemo(() => {
    const needle = q.trim().toLowerCase();
    return (needle ? models.filter((m) => m.name.toLowerCase().includes(needle)) : models).slice(0, 50);
  }, [models, q]);
  // Index 0 ist immer "Kein Modell", danach die Treffer - eine gemeinsame
  // Liste fuer die Pfeiltasten-Navigation.
  const optionIds: OptionId[] = [NONE, ...hits.map((m) => m.id)];

  useEffect(() => setActive(0), [q]);

  const choose = (id: OptionId) => onChange(id === NONE ? null : id);
  const domId = (id: OptionId) => (id === NONE ? `${uid}-none` : `${uid}-${id}`);
  const activeId = optionIds[Math.min(active, optionIds.length - 1)];

  return (
    <div
      className="absolute z-50 mt-1 w-[280px] rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] p-1.5 flex flex-col gap-1"
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
        autoFocus
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
    </div>
  );
}
