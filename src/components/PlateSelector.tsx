import { Icon } from './Icon';
import { useT } from '../i18n/LanguageContext';
import type { GeometryPlate } from '../lib/parseModelGeometry';

const focusClass = 'focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]';

export function PlateSelector({ plates, selected, onSelect }: {
  plates: GeometryPlate[]; selected: number | null; onSelect: (number: number | null) => void;
}) {
  const t = useT();
  const selectedIndex = plates.findIndex(plate => plate.number === selected);
  const label = (plate: GeometryPlate) => `${t('viewerPlate').replace('{n}', String(plate.number))}${plate.name ? ` · ${plate.name}` : ''}`;
  const plate = plates[selectedIndex];
  const previousDisabled = selectedIndex <= 0;
  const nextDisabled = selectedIndex === plates.length - 1;
  const previous = () => { if (!previousDisabled) onSelect(plates[selectedIndex - 1].number); };
  const next = () => { if (!nextDisabled) onSelect(plates[selectedIndex + 1].number); };

  if (plates.length < 2) return null;

  return <div role="group" aria-label={t('viewerPrintPlate')} data-plate-selector
    className="plate-selector min-w-0 shrink-0"
    onKeyDown={event => {
      event.stopPropagation();
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight') {
        event.preventDefault();
        if (event.key === 'ArrowLeft') previous();
        else next();
      }
    }}>
    <button type="button" className={`viewer-pill ${focusClass}`} aria-pressed={selected === null}
      onClick={() => onSelect(null)}>{t('viewerAllPlates')}</button>
    <button type="button" className={`viewer-pill plate-arrow ${focusClass}`} aria-label={t('viewerPreviousPlate')}
      aria-disabled={previousDisabled} onClick={previous}><Icon name="previous" size={24} /></button>
    {plates.length <= 12 ? <span className="plate-numbers">{plates.map(item => <button key={item.number} type="button"
      className={`viewer-pill plate-number font-code ${focusClass}`} aria-label={label(item)} title={label(item)}
      aria-current={selected === item.number ? 'true' : undefined}
      onClick={() => onSelect(item.number)}>{item.number}</button>)}</span> :
      <span className="plate-selector-summary">
        <span className="plate-selector-count">{plate ? `${selectedIndex + 1} / ${plates.length}` : t('viewerAllPlates')}</span>
        {plate?.name && <span className="plate-selector-name" title={plate.name}>{plate.name}</span>}
      </span>}
    <button type="button" className={`viewer-pill plate-arrow ${focusClass}`} aria-label={t('viewerNextPlate')}
      aria-disabled={nextDisabled} onClick={next}><Icon name="next" size={24} /></button>
    <span role="status" aria-live="polite" aria-atomic="true" className="sr-only">
      {plate ? label(plate) : t('viewerAllPlatesAnnouncement')}
    </span>
  </div>;
}
