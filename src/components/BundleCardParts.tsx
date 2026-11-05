import { useCallback } from 'react';
import { useCanEditMetadata } from '../lib/webSession';
import { displayName } from '../lib/displayName';
import { formatWeightG } from '../i18n/format';
import { useLanguage, useT, useFormatCount } from '../i18n/LanguageContext';
import { bundleCardMeta, bundleCardSummary } from '../lib/bundleCards';
import type { BundleCard } from '../lib/bundles';
import { BulkCheckbox } from './BulkCheckbox';
import { Icon } from './Icon';
import type { HTMLAttributes } from 'react';

/** Texts of a bundle card: "3 variants", "3 parts · 2 of 3 printed", and the accessible name built from them. */
export function useBundleText() {
  const t = useT();
  const formatCount = useFormatCount();
  const summary = useCallback((card: BundleCard) => {
    const { kind, shown, total, printed } = bundleCardSummary(card);
    const whole = shown === total;
    const forms = kind === 'variants'
      ? t(whole ? 'bundleCountVariants' : 'bundleCountVariantsPartial')
      : t(whole ? 'bundleCountParts' : 'bundleCountPartsPartial');
    const count = formatCount(forms, total).replace('{shown}', String(shown));
    if (kind === 'variants') return count;
    // Parts: how far the print is. With a filter only the shown parts are counted.
    const progress = whole
      ? t('bundlePrintedOf').replace('{printed}', String(printed)).replace('{total}', String(total))
      : t('bundlePrintedShown').replace('{printed}', String(printed));
    return `${count} · ${progress}`;
  }, [t, formatCount]);
  const ariaLabel = useCallback((card: BundleCard) =>
    t('bundleCardAria').replace('{name}', displayName(card.bundle.name)).replace('{summary}', summary(card)), [t, summary]);
  return { summary, ariaLabel };
}

/** What a file marked as part of a bundle says: "Part of ..." in the catalog, "was part of bundle ..." in the trash. */
export function usePartOfText(trash: boolean) {
  const t = useT();
  return useCallback((bundleName: string) =>
    t(trash ? 'bundleWasPartOf' : 'bundlePartOf').replace('{name}', displayName(bundleName)), [t, trash]);
}

/** Row of a single card that stands for the tags: the file belongs to a bundle that is not shown as a card. */
export function PartOfChip({ text, comfort }: { text: string; comfort: boolean }) {
  return <div data-part-of className="flex items-center shrink-0 min-w-0" style={{ height: comfort ? 28 : 21 }}>
    <span title={text}
      className="truncate max-w-full font-semibold text-caption px-1.5 rounded-full border border-dashed border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
      style={{ lineHeight: '15px', paddingTop: 1, paddingBottom: 1 }}>
      {text}
    </span>
  </div>;
}

interface BundleGridCardProps {
  card: BundleCard;
  comfort: boolean;
  /** Row attributes of the grid (role, index, selection state). */
  rowProps: HTMLAttributes<HTMLDivElement>;
  open: boolean;
  image: string | null;
  checked: boolean;
  readOnly?: boolean;
  /** Height the metadata block has to keep so every card of the grid is equally high. */
  metadataHeight: number;
  onSelect: () => void;
  onOpenDetail: () => void;
  onContextMenu: (x: number, y: number) => void;
  onToggleChecked: () => void;
}

/**
 * Card of a bundle. It has the outer size of a single card: the "stack" is only an edge strip
 * inside the picture, the line under the name takes the place of the tags.
 */
export function BundleGridCard({ card, comfort, rowProps, open, image, checked, readOnly, metadataHeight, onSelect, onOpenDetail, onContextMenu, onToggleChecked }: BundleGridCardProps) {
  const { language } = useLanguage();
  const canEdit = useCanEditMetadata();
  const text = useBundleText();
  const meta = bundleCardMeta(card);
  const summary = text.summary(card);
  const name = displayName(card.bundle.name);
  return (
    <div
      data-model-id={card.title.id}
      data-bundle-id={card.bundle.id}
      {...rowProps}
      aria-label={text.ariaLabel(card)}
      tabIndex={-1}
      onClick={onSelect}
      onDoubleClick={readOnly ? undefined : onOpenDetail}
      onContextMenu={(e) => {
        e.preventDefault();
        onSelect();
        onContextMenu(e.clientX, e.clientY);
      }}
      className={comfort
        ? `hover:border-[var(--accent)] group rounded-[10px] overflow-hidden cursor-pointer bg-[var(--panel)] shadow-[var(--shadow)] border-2 ${open ? 'border-[var(--accent)]' : 'border-transparent'}`
        : `hover:border-[var(--accent)] group rounded-[10px] overflow-hidden border cursor-pointer ${open ? 'border-[var(--accent)]' : 'border-[var(--line)]'}`}
    >
      {comfort && <div aria-hidden="true" className="h-[5px]" style={{ background: 'linear-gradient(90deg, var(--accent), var(--accent-soft))' }} />}
      <div role="gridcell" className={`relative aspect-square bg-[var(--plate)] overflow-hidden ${comfort ? '' : 'border-b border-[var(--line)]'}`}>
        {!readOnly && canEdit && (
          <BulkCheckbox checked={checked} onToggle={onToggleChecked} className="absolute top-1.5 right-6 z-10" />
        )}
        {image ? (
          <img draggable={false} src={image} alt="" className="absolute inset-0 w-full h-full object-cover" />
        ) : (
          <>
            <div className="absolute inset-0 opacity-90"
              style={{ backgroundImage: `repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px ${comfort ? 12 : 9}px)` }} />
            <div className="absolute inset-0 grid place-items-center">
              <div className={`${comfort ? 'w-[64px] h-[64px]' : 'w-[52px] h-[52px]'} border border-dashed border-[var(--line-strong)] rotate-45`} />
            </div>
          </>
        )}
        <span data-bundle-count aria-hidden="true"
          className={`absolute z-[1] inline-flex items-center gap-0.5 font-bold text-caption border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] ${comfort ? 'left-2.5 top-2.5 px-2 py-0.5 rounded-lg' : 'left-[7px] top-[7px] px-1.5 py-px rounded'}`}>
          <Icon name="layers" size={13} />{card.visibleMembers.length}
        </span>
        <span data-bundle-edge aria-hidden="true"
          className="absolute z-[1] top-0 right-0 bottom-0 w-[10px] pointer-events-none border-l border-[var(--line-strong)] bg-[var(--panel-2)]"
          style={{ boxShadow: 'inset 4px 0 0 var(--plate), inset 5px 0 0 var(--line-strong)' }} />
      </div>
      <div data-card-metadata role="gridcell" className={`flex flex-col ${comfort ? 'gap-2' : 'gap-1.5 px-2.5 py-2.5 bg-[var(--panel)]'}`}
        style={{ ...(comfort ? { padding: 'var(--space-card-pad)' } : {}), height: metadataHeight }}>
        <div title={name} className={comfort ? 'select-none font-bold overflow-hidden text-ellipsis whitespace-nowrap shrink-0' : 'select-none text-small font-semibold overflow-hidden text-ellipsis whitespace-nowrap shrink-0'}
          style={comfort ? { fontSize: 'var(--font-size-title)', height: 26, lineHeight: '26px' } : { height: 19, lineHeight: '19px' }}>
          {name}
        </div>
        <div className="flex items-center gap-3 text-[var(--ink-3)] font-medium overflow-hidden whitespace-nowrap shrink-0"
          style={{ height: comfort ? 20 : 15, lineHeight: comfort ? '20px' : '15px', fontSize: 'var(--font-size-meta)' }}>
          {meta.materials.length > 0 && <span className="truncate"><Icon name="box" size={14} /> {meta.materials.join(', ')}</span>}
          {meta.weightG !== null && <span className="shrink-0"><Icon name="weight" size={14} /> {meta.weightFromSlicer
            ? formatWeightG(meta.weightG, language)
            : `≈ ${formatWeightG(meta.weightG, language)}`}</span>}
        </div>
        <div data-bundle-line title={summary}
          className="flex items-center gap-1.5 shrink-0 min-w-0 font-semibold text-caption text-[var(--ink-2)] overflow-hidden whitespace-nowrap"
          style={{ height: comfort ? 28 : 21 }}>
          <span className="text-[var(--accent)] flex-none"><Icon name="layers" size={14} /></span>
          <span className="truncate">{summary}</span>
        </div>
      </div>
    </div>
  );
}
