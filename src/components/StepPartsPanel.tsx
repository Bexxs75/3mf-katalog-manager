import { useId, useState, type KeyboardEvent } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatPartSize, MAX_STEP_PARTS, type StepPart } from '../lib/stepParts';
import { Icon } from './Icon';

interface Props {
  parts: StepPart[];
  hidden: ReadonlySet<number>;
  selected: number | null;
  onToggle: (index: number) => void;
  onSelect: (index: number) => void;
  onShowAll: () => void;
  /** Highlight in the picture (hover or focus); null when it ends. */
  onHighlight: (index: number | null) => void;
  /** Save actions; left out where the file has no catalog id (view only). */
  onSave?: (what: 'selected' | 'visible' | 'each') => void;
}

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';
const smallButton = `h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-small font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-default ${focusClass}`;

/** Parts list of a STEP file in the viewer: visibility, highlight, size of the chosen part and the STL actions. */
export function StepPartsPanel({ parts, hidden, selected, onToggle, onSelect, onShowAll, onHighlight, onSave }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const listId = useId();
  const [open, setOpen] = useState(true);
  const name = (part: StepPart) => part.rest ? t('spRest') : t('spPart').replace('{n}', String(part.index + 1));
  const chosen = selected === null ? undefined : parts.find(part => part.index === selected);

  // The list is a group of checkboxes: arrow keys move between them.
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    event.stopPropagation();
    if (event.key !== 'ArrowDown' && event.key !== 'ArrowUp') return;
    const boxes = Array.from(event.currentTarget.querySelectorAll<HTMLInputElement>('input[type="checkbox"]'));
    const at = boxes.indexOf(document.activeElement as HTMLInputElement);
    if (at === -1) return;
    event.preventDefault();
    boxes[Math.min(boxes.length - 1, Math.max(0, at + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus();
  };

  return <div data-step-parts className="absolute right-3 top-3 w-[min(250px,calc(100%-24px))] max-h-[calc(100%-24px)] flex flex-col overflow-hidden bg-[var(--panel)] text-[var(--ink)] border border-[var(--line)] rounded-lg shadow-[var(--shadow)] z-[2]">
    <div className="flex items-center gap-1.5 px-2 py-1.5">
      <button type="button" aria-expanded={open} aria-controls={listId} onClick={() => setOpen(value => !value)}
        className={`flex-1 min-w-0 flex items-center gap-1.5 bg-transparent border-0 p-0.5 text-left text-small font-semibold cursor-pointer text-[var(--ink)] ${focusClass}`}>
        <Icon name="chevron" size={14} className={open ? 'rotate-90' : ''} />{t('spTitle').replace('{n}', String(parts.length))}
      </button>
      {open && <button type="button" className={smallButton} disabled={hidden.size === 0} onClick={onShowAll}>{t('spShowAll')}</button>}
    </div>
    {open && <div id={listId}>
      <div role="group" aria-label={t('spGroup')} onKeyDown={onKeyDown} className="overflow-y-auto max-h-[240px] px-1 py-1 border-t border-[var(--line)]">
        {parts.map(part => {
          const shown = !hidden.has(part.index);
          const state = shown ? t('spVisible') : t('spHidden');
          const label = part.rest ? `${name(part)}, ${state}`
            : t('spRowLabel').replace('{name}', name(part)).replace('{total}', String(parts.length)).replace('{state}', state);
          return <div key={part.index} data-part-row={part.index}
            className={`flex items-center gap-1.5 px-1 rounded-[5px] hover:bg-[var(--accent-soft)] ${selected === part.index ? 'outline outline-1 outline-[var(--accent)]' : ''}`}
            onMouseEnter={() => onHighlight(part.index)} onMouseLeave={() => onHighlight(null)}
            onFocus={() => onHighlight(part.index)} onBlur={() => onHighlight(null)}>
            <input type="checkbox" checked={shown} aria-label={label} onChange={() => onToggle(part.index)} className="accent-[var(--accent)]" />
            <button type="button" aria-pressed={selected === part.index} onClick={() => onSelect(part.index)}
              title={part.rest ? t('spRestHint').replace('{n}', String(MAX_STEP_PARTS)) : undefined}
              className={`flex-1 min-w-0 truncate text-left bg-transparent border-0 py-1 text-small cursor-pointer ${focusClass} ${shown ? 'text-[var(--ink)]' : 'text-[var(--ink-3)] line-through'}`}>
              {name(part)}
            </button>
          </div>;
        })}
      </div>
      <div className="flex flex-col gap-1.5 p-2 border-t border-[var(--line)] text-small">
        <div role="status">{chosen
          ? <><b>{name(chosen)}</b><br />{t('spSize').replace('{size}', formatPartSize(chosen.size, language))}</>
          : t('spNoneSelected')}</div>
        {onSave && <>
          <button type="button" className={smallButton} disabled={!chosen} onClick={() => onSave('selected')}>
            {chosen ? t('spSaveOne').replace('{name}', name(chosen)) : t('spSaveSelected')}
          </button>
          {hidden.size > 0 && hidden.size < parts.length && <button type="button" className={smallButton} onClick={() => onSave('visible')}>{t('spSaveVisible')}</button>}
          <button type="button" className={smallButton} onClick={() => onSave('each')}>{t('spSaveEach')}</button>
        </>}
      </div>
    </div>}
  </div>;
}
