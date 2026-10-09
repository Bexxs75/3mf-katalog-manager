import { useEffect, useId, useRef, useState } from 'react';
import { Icon } from './Icon';
import { useT } from '../i18n/LanguageContext';
import type { GeometryPlate } from '../lib/parseModelGeometry';

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';

export function PlateSelector({ plates, selected, onSelect }: {
  plates: GeometryPlate[]; selected: number | null; onSelect: (number: number | null) => void;
}) {
  const t = useT();
  const id = useId();
  const root = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const list = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const selectedIndex = plates.findIndex(plate => plate.number === selected);
  const activeIndex = Math.min(active, plates.length - 1);
  const label = (plate: GeometryPlate) => t('viewerPlate').replace('{n}', String(plate.number));
  const plate = plates[selectedIndex];
  const title = plate ? `${label(plate)}${plate.name ? ` · ${plate.name}` : ''}` : t('viewerChoosePlate');

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: Event) => {
      if (event.target instanceof Node && !root.current?.contains(event.target)) setOpen(false);
    };
    document.addEventListener('pointerdown', dismiss);
    document.addEventListener('click', dismiss);
    list.current?.focus();
    return () => {
      document.removeEventListener('pointerdown', dismiss);
      document.removeEventListener('click', dismiss);
    };
  }, [open]);
  useEffect(() => {
    // Keep keyboard navigation visible even in files with many plates.
    if (open) list.current?.children[activeIndex]?.scrollIntoView?.({ block: 'nearest' });
  }, [open, activeIndex]);

  if (plates.length < 2) return null;
  const showList = () => { setActive(Math.max(0, selectedIndex)); setOpen(true); };
  const closeList = () => { setOpen(false); button.current?.focus(); };
  const choose = (number: number) => { onSelect(number); closeList(); };

  return <div ref={root} role="group" aria-label={t('viewerPrintPlate')} data-plate-selector
    className="plate-selector relative flex flex-nowrap items-center w-full min-w-0 shrink-0"
    onKeyDown={event => event.stopPropagation()}
    onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false); }}>
    <button type="button" className={`viewer-pill ${focusClass}`} aria-pressed={selected === null}
      onClick={() => { onSelect(null); setOpen(false); }}>{t('viewerAllPlates')}</button>
    <button type="button" className={`viewer-pill plate-arrow ${focusClass}`} aria-label={t('viewerPreviousPlate')}
      disabled={selectedIndex <= 0} onClick={() => { onSelect(plates[selectedIndex - 1].number); setOpen(false); }}>‹</button>
    <button ref={button} type="button" className={`viewer-pill plate-selector-trigger ${focusClass}`} title={title}
      aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? id : undefined}
      onClick={() => open ? closeList() : showList()}
      onKeyDown={event => {
        if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); showList(); }
        if (event.key === 'Escape') { event.preventDefault(); closeList(); }
      }}>
      <span className="plate-selector-label">{title}</span><Icon name="chevron" size={14} className="shrink-0" />
    </button>
    <button type="button" className={`viewer-pill plate-arrow ${focusClass}`} aria-label={t('viewerNextPlate')}
      disabled={selectedIndex === plates.length - 1} onClick={() => { onSelect(plates[selectedIndex + 1].number); setOpen(false); }}>›</button>
    {open && <div ref={list} id={id} role="listbox" tabIndex={-1} aria-label={t('viewerPrintPlate')}
      aria-activedescendant={`${id}-${activeIndex}`}
      className={`plate-selector-list absolute overflow-y-auto ${focusClass}`}
      onKeyDown={event => {
        switch (event.key) {
          case 'ArrowDown': event.preventDefault(); setActive(Math.min(activeIndex + 1, plates.length - 1)); break;
          case 'ArrowUp': event.preventDefault(); setActive(Math.max(activeIndex - 1, 0)); break;
          case 'Home': event.preventDefault(); setActive(0); break;
          case 'End': event.preventDefault(); setActive(plates.length - 1); break;
          case 'Enter': event.preventDefault(); choose(plates[activeIndex].number); break;
          case 'Escape': event.preventDefault(); closeList(); break;
        }
      }}>
      {plates.map((item, index) => <div key={item.number} id={`${id}-${index}`} role="option"
        aria-selected={selected === item.number} data-active={activeIndex === index}
        className="plate-selector-option" onMouseDown={event => event.preventDefault()} onClick={() => choose(item.number)}>
        <span>{label(item)}</span>{' '}
        {item.name && <small title={item.name}>{item.name}</small>}
      </div>)}
    </div>}
  </div>;
}
