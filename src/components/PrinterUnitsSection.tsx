import { useState } from 'react';
import { Icon } from './Icon';
import { useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';
import { UNIT_TEMPLATES, isValidColorHex } from '../lib/filamentColors';
import type { FilamentSpool, MaterialUnit, Printer, UnitKind } from '../types';
import type { PrintersState } from '../hooks/usePrinters';
import { usePrinterReorder } from '../hooks/usePrinterReorder';
import { PrinterManagerDialog, pmButton, pmField } from './PrinterManagerDialog';

type KindLabelKey =
  | 'printersKindBambuAms'
  | 'printersKindBambuAmsLite'
  | 'printersKindBambuAmsHt'
  | 'printersKindCrealityCfs'
  | 'printersKindPrusaMmu3'
  | 'printersKindAnycubicAce'
  | 'printersKindExternal'
  | 'printersKindCustom'
  | 'printersKindResinVat';

const KIND_LABEL: Record<UnitKind, KindLabelKey> = {
  bambu_ams: 'printersKindBambuAms',
  bambu_ams_lite: 'printersKindBambuAmsLite',
  bambu_ams_ht: 'printersKindBambuAmsHt',
  creality_cfs: 'printersKindCrealityCfs',
  prusa_mmu3: 'printersKindPrusaMmu3',
  anycubic_ace: 'printersKindAnycubicAce',
  external: 'printersKindExternal',
  custom: 'printersKindCustom',
  resin_vat: 'printersKindResinVat',
};

const LETTERS = 'ABCDEFGHIJKLMNOP';

/** "AMS A", "AMS B" … or "Spulenhalter", "Spulenhalter 2" … for new units. */
export function suggestUnitName(printer: Printer, kind: UnitKind, defaultName: string): string {
  const sameKind = printer.units.filter((u) => u.kind === kind).length;
  if (kind === 'bambu_ams' || kind === 'bambu_ams_lite') return `${defaultName} ${LETTERS[sameKind] ?? sameKind + 1}`;
  return sameKind === 0 ? defaultName : `${defaultName} ${sameKind + 1}`;
}


export function PrinterUnitsSection({ printer, spools, actions, onChanged, onMaterial }: {
  printer: Printer; spools: FilamentSpool[]; actions: PrintersState;
  onChanged: () => void; onMaterial: () => void;
}) {
  const t = useT();
  const [editing, setEditing] = useState<MaterialUnit | 'new' | null>(null);
  const [deleting, setDeleting] = useState<MaterialUnit | null>(null);
  const order = usePrinterReorder(printer.units.map(u => u.id), ids => actions.reorderUnits(printer.id, ids));
  const count = deleting ? spools.filter(s => s.unitId === deleting.id).length : 0;
  return <>
    {printer.units.map(unit => <div key={unit.id} onMouseEnter={() => order.enter(unit.id)}
      className={`group/unit flex flex-wrap items-center gap-3 py-2 border-b ${order.over === unit.id ? 'border-[var(--sel-line)]' : 'border-[var(--line)]'}`}>
      <div className="flex items-center gap-2 min-w-0 flex-1">
        {unit.kind !== 'resin_vat' && <button type="button" className="text-[var(--ink-3)] cursor-grab opacity-0 group-hover/unit:opacity-80 focus-visible:opacity-100 focus-visible:outline-2"
          aria-label={`${t('printersReorderHint')} ${unit.name}`} onMouseDown={e => { if (e.button === 0) order.begin(e, unit.id); }} onKeyDown={e => order.key(e, unit.id)}>⋮⋮</button>}
        <Icon name={unit.kind === 'external' ? 'spool' : unit.kind === 'resin_vat' ? 'resin' : 'ams'} size={24} className="shrink-0 text-[var(--ink-2)]" />
        <div className="min-w-0"><b className="block break-words">{unit.name}</b>
          <small className="block text-[var(--ink-3)]">{t(KIND_LABEL[unit.kind])} · {formatCount(t('printersUnitSlotCount'), unit.slotCount)} · {t('pmOccupied').replace('{count}', String(spools.filter(s => s.unitId === unit.id && s.slotIndex !== null && s.slotIndex >= 0 && s.slotIndex < unit.slotCount).length))}</small>
        </div>
      </div>
      <div className="flex flex-wrap gap-1.5">{Array.from({ length: Math.min(16, unit.slotCount) }, (_, i) => {
        const spool = spools.find(s => s.unitId === unit.id && s.slotIndex === i);
        const label = `${i + 1} · ${spool ? [spool.material, spool.color].filter(Boolean).join(' · ') : t('printersSlotEmpty')}`;
        return <span key={i} title={label} aria-label={label} className="w-6 h-6 rounded-full border border-[var(--line-strong)] shrink-0"
          style={{ background: !spool ? 'repeating-linear-gradient(45deg, transparent 0 3px, var(--line) 3px 4px)' : spool.colorHex && isValidColorHex(spool.colorHex) ? spool.colorHex : 'var(--unk-soft)' }} />;
      })}</div>
      {unit.kind !== 'resin_vat' && <div className="flex flex-wrap gap-1.5 max-[639px]:w-full">
        <button className={`${pmButton} !border-transparent hover:bg-[var(--panel-2)] max-[639px]:w-full`} onClick={() => setEditing(unit)}>{t('pmEdit')}</button>
        <button className={`${pmButton} !border-transparent hover:bg-[var(--panel-2)] text-[var(--ink-2)] max-[639px]:w-full`} aria-label={`${t('printersDelete')} ${unit.name}`} onClick={() => setDeleting(unit)}>{t('printersDelete')}</button>
      </div>}
    </div>)}
    <div className="flex flex-wrap items-center justify-between gap-3 mt-2">
      <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] text-[var(--accent)] focus-visible:outline-2 max-[639px]:w-full" onClick={onMaterial}>{t('pmLoadSpools')}</button>
      {printer.kind !== 'resin' && <button className={`${pmButton} !border-transparent hover:bg-[var(--panel-2)] max-[639px]:w-full`} onClick={() => setEditing('new')}>+ {t('printersAddUnitButton')}</button>}
    </div>
    {printer.kind === 'resin' && <p className="text-[var(--ink-3)] mt-2">{t('printersResinPrinterNote')}</p>}
    {editing && <UnitDialog printer={printer} unit={editing === 'new' ? null : editing} actions={actions}
      onClose={() => setEditing(null)} onChanged={onChanged} />}
    {deleting && <PrinterManagerDialog title={t('printersDeleteUnitConfirm').replace('{name}', deleting.name)}
      onClose={() => setDeleting(null)} submitLabel={t('printersDelete')}
      onSubmit={async () => { await actions.deleteUnit(deleting.id); onChanged(); }}>
      {count > 0 && <p>{formatCount(t('printersDeleteReturnHomeCount'), count)}</p>}
    </PrinterManagerDialog>}
  </>;
}

function UnitDialog({ printer, unit, actions, onClose, onChanged }: {
  printer: Printer; unit: MaterialUnit | null; actions: PrintersState; onClose: () => void; onChanged: () => void;
}) {
  const t = useT();
  const [kind, setKind] = useState<UnitKind>(unit?.kind ?? 'bambu_ams');
  const [name, setName] = useState(unit?.name ?? suggestUnitName(printer, 'bambu_ams', 'AMS'));
  const [slots, setSlots] = useState(unit?.slotCount ?? 4);
  return <PrinterManagerDialog title={unit ? t('pmEdit') : t('printersAddUnitButton')} onClose={onClose}
    submitLabel={unit ? t('printersSave') : t('printersAddUnitButton')} onSubmit={async () => {
      if (unit) await actions.updateUnit(unit.id, name.trim(), kind === 'custom' ? slots : null);
      else await actions.addUnit(printer.id, kind, name.trim(), kind === 'custom' ? slots : null);
      onChanged();
    }}>
    {!unit && <div role="group" aria-label={t('pmKind')} className="flex flex-wrap gap-2">{UNIT_TEMPLATES.map(template =>
      <button type="button" key={template.kind} className={`${pmButton} ${kind === template.kind ? 'border-[var(--accent)] text-[var(--accent)]' : ''}`} aria-pressed={kind === template.kind} onClick={() => {
        setKind(template.kind);
        setName(suggestUnitName(printer, template.kind, template.defaultName || t(KIND_LABEL[template.kind])));
        setSlots(template.slotCount ?? 4);
      }}>{t(KIND_LABEL[template.kind])} · {template.slotCount ?? '1–16'}</button>)}</div>}
    <label>{t('pmName')}<input required maxLength={60} className={pmField} value={name} onChange={e => setName(e.target.value)} /></label>
    {kind === 'custom' && <label>{t('printersSlotsLabel')}<input type="number" required min={1} max={16} step={1}
      className={pmField} value={slots} onChange={e => setSlots(Number(e.target.value))} /></label>}
    <p className="text-[var(--ink-3)]">{t('pmUnitsHint')}</p>
  </PrinterManagerDialog>;
}
