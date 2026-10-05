import { useT } from '../i18n/LanguageContext';
import type { GeometryPlate } from '../lib/parseModelGeometry';

export function PlateSelector({ plates, selected, onSelect }: {
  plates: GeometryPlate[]; selected: number | null; onSelect: (number: number | null) => void;
}) {
  const t = useT();
  if (plates.length < 2) return null;
  const options = [{ number: null, name: null }, ...plates];
  return <div role="radiogroup" aria-label={t('viewerPrintPlate')}
    className="viewer-toggle w-full justify-start overflow-x-auto shrink-0">
    {options.map((plate, index) => <button key={plate.number ?? 'all'} role="radio"
      aria-checked={selected === plate.number} tabIndex={selected === plate.number ? 0 : -1}
      title={plate.name ?? undefined} onClick={() => onSelect(plate.number)}
      className="shrink-0 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)]"
      onKeyDown={event => {
        const direction = event.key === 'ArrowRight' || event.key === 'ArrowDown' ? 1
          : event.key === 'ArrowLeft' || event.key === 'ArrowUp' ? -1 : 0;
        if (!direction && event.key !== 'Home' && event.key !== 'End') return;
        event.preventDefault(); event.stopPropagation();
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? options.length - 1
          : (index + direction + options.length) % options.length;
        onSelect(options[next].number);
        const buttons = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]');
        buttons?.[next].focus();
      }}>
      {plate.number === null ? t('viewerAllPlates') : t('viewerPlate').replace('{n}', String(plate.number))}
    </button>)}
  </div>;
}
