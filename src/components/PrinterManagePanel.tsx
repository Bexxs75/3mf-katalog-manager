import { useEffect, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { formatCount } from '../i18n/types';
import { UNIT_TEMPLATES } from '../lib/filamentColors';
import type { FilamentSpool, MaterialUnit, Printer, UnitKind } from '../types';

export interface PrinterActions {
  addPrinter: (name: string) => Promise<unknown>;
  renamePrinter: (printerId: string, name: string) => Promise<unknown>;
  deletePrinter: (printerId: string) => Promise<number>;
  addUnit: (printerId: string, kind: UnitKind, name: string, slotCount: number | null) => Promise<unknown>;
  updateUnit: (unitId: string, name: string, slotCount: number | null) => Promise<number>;
  deleteUnit: (unitId: string) => Promise<number>;
  reorderUnits: (printerId: string, unitIds: string[]) => Promise<unknown>;
}

interface Props {
  open: boolean;
  printers: Printer[];
  spools: FilamentSpool[];
  error: string | null;
  actions: PrinterActions;
  onClose: () => void;
  /** Nach Aenderungen, die Spulen an ihren Stammplatz zurueckschicken koennen. */
  onSpoolsChanged: () => void;
}

type KindLabelKey =
  | 'printersKindBambuAms'
  | 'printersKindBambuAmsLite'
  | 'printersKindBambuAmsHt'
  | 'printersKindCrealityCfs'
  | 'printersKindPrusaMmu3'
  | 'printersKindAnycubicAce'
  | 'printersKindExternal'
  | 'printersKindCustom';

const KIND_LABEL: Record<UnitKind, KindLabelKey> = {
  bambu_ams: 'printersKindBambuAms',
  bambu_ams_lite: 'printersKindBambuAmsLite',
  bambu_ams_ht: 'printersKindBambuAmsHt',
  creality_cfs: 'printersKindCrealityCfs',
  prusa_mmu3: 'printersKindPrusaMmu3',
  anycubic_ace: 'printersKindAnycubicAce',
  external: 'printersKindExternal',
  custom: 'printersKindCustom',
};

const LETTERS = 'ABCDEFGHIJKLMNOP';

/** "AMS A", "AMS B" … bzw. "Extern", "Extern 2" … fuer neue Einheiten. */
export function suggestUnitName(printer: Printer, kind: UnitKind, defaultName: string): string {
  const sameKind = printer.units.filter((u) => u.kind === kind).length;
  if (kind === 'bambu_ams' || kind === 'bambu_ams_lite') return `${defaultName} ${LETTERS[sameKind] ?? sameKind + 1}`;
  return sameKind === 0 ? defaultName : `${defaultName} ${sameKind + 1}`;
}

const fieldClass =
  'h-8 px-2 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] outline-0 text-[12.5px] focus:border-[var(--accent)]';
const smallButton =
  'h-7 px-2 rounded-[4px] border border-[var(--line-strong)] text-[11.5px] text-[var(--ink-2)] hover:border-[var(--accent)] cursor-pointer';
const primaryButton =
  'h-8 px-3 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-bold cursor-pointer disabled:opacity-50';

function Stepper({ value, onChange, label }: { value: number; onChange: (n: number) => void; label: string }) {
  return (
    <span className="inline-flex items-center gap-1" aria-label={label}>
      <button type="button" className={smallButton} onClick={() => onChange(Math.max(1, value - 1))} aria-label="−">
        −
      </button>
      <span className="font-mono-ui text-[12px] w-6 text-center">{value}</span>
      <button type="button" className={smallButton} onClick={() => onChange(Math.min(16, value + 1))} aria-label="+">
        +
      </button>
    </span>
  );
}

/** Seitenpanel zum Anlegen und Pflegen von Druckern und ihren Einheiten. */
export function PrinterManagePanel({ open, printers, spools, error, actions, onClose, onSpoolsChanged }: Props) {
  const t = useT();
  const [newPrinter, setNewPrinter] = useState('');
  const [renaming, setRenaming] = useState<{ id: string; kind: 'printer' | 'unit'; value: string } | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<{ id: string; kind: 'printer' | 'unit' } | null>(null);
  const [addMenuFor, setAddMenuFor] = useState<string | null>(null);
  const [customFor, setCustomFor] = useState<{ printerId: string; name: string; slots: number } | null>(null);
  const [dragUnit, setDragUnit] = useState<{ printerId: string; unitId: string } | null>(null);
  const [dragOverUnit, setDragOverUnit] = useState<string | null>(null);

  useEffect(() => {
    if (!open) {
      setRenaming(null);
      setConfirmDelete(null);
      setAddMenuFor(null);
      setCustomFor(null);
    }
  }, [open]);

  // Sortieren per Ziehen am Griff: zeigerbasiert wie im restlichen Projekt
  // (kein natives HTML5-Drag-&-Drop unter Tauri/WebKitGTK).
  useEffect(() => {
    if (!dragUnit) return;
    const handleUp = () => {
      const over = dragOverUnit;
      const dragged = dragUnit;
      setDragUnit(null);
      setDragOverUnit(null);
      if (!over || over === dragged.unitId) return;
      const printer = printers.find((p) => p.id === dragged.printerId);
      if (!printer) return;
      const ids = printer.units.map((u) => u.id);
      const from = ids.indexOf(dragged.unitId);
      const to = ids.indexOf(over);
      if (from < 0 || to < 0) return;
      ids.splice(from, 1);
      ids.splice(to, 0, dragged.unitId);
      actions.reorderUnits(printer.id, ids).catch(() => {});
    };
    document.addEventListener('mouseup', handleUp);
    return () => document.removeEventListener('mouseup', handleUp);
  }, [dragUnit, dragOverUnit, printers, actions]);

  const spoolsIn = (unitIds: string[]) => spools.filter((s) => s.unitId !== null && unitIds.includes(s.unitId)).length;

  const submitNewPrinter = () => {
    const name = newPrinter.trim();
    if (!name) return;
    actions.addPrinter(name).then(() => setNewPrinter(''), () => {});
  };

  const submitRename = () => {
    if (!renaming || !renaming.value.trim()) return;
    const done = () => setRenaming(null);
    if (renaming.kind === 'printer') {
      actions.renamePrinter(renaming.id, renaming.value.trim()).then(done, () => {});
    } else {
      const unit = printers.flatMap((p) => p.units).find((u) => u.id === renaming.id);
      if (!unit) return;
      actions.updateUnit(unit.id, renaming.value.trim(), null).then(done, () => {});
    }
  };

  const runDelete = () => {
    if (!confirmDelete) return;
    const op =
      confirmDelete.kind === 'printer' ? actions.deletePrinter(confirmDelete.id) : actions.deleteUnit(confirmDelete.id);
    op.then(() => {
      setConfirmDelete(null);
      onSpoolsChanged();
    }, () => {});
  };

  const addTemplate = (printer: Printer, kind: UnitKind) => {
    setAddMenuFor(null);
    const template = UNIT_TEMPLATES.find((tpl) => tpl.kind === kind);
    if (!template) return;
    if (template.slotCount === null) {
      setCustomFor({ printerId: printer.id, name: '', slots: 4 });
      return;
    }
    actions.addUnit(printer.id, kind, suggestUnitName(printer, kind, template.defaultName), null).catch(() => {});
  };

  const changeCustomSlots = (unit: MaterialUnit, slots: number) => {
    actions.updateUnit(unit.id, unit.name, slots).then((returned) => {
      if (returned > 0) onSpoolsChanged();
    }, () => {});
  };

  const renameRow = (id: string, kind: 'printer' | 'unit') =>
    renaming?.id === id && renaming.kind === kind ? (
      <span className="flex items-center gap-1 flex-1">
        <input
          autoFocus
          value={renaming.value}
          onChange={(e) => setRenaming({ ...renaming, value: e.target.value })}
          onKeyDown={(e) => {
            if (e.key === 'Enter') submitRename();
            if (e.key === 'Escape') setRenaming(null);
          }}
          className={`${fieldClass} flex-1 min-w-0`}
        />
        <button type="button" className={smallButton} onClick={submitRename}>
          {t('printersSave')}
        </button>
      </span>
    ) : null;

  const confirmRow = (id: string, kind: 'printer' | 'unit', text: string) =>
    confirmDelete?.id === id && confirmDelete.kind === kind ? (
      <div className="flex items-center gap-1.5 mt-1.5 text-[11.5px]">
        <span className="flex-1">{text}</span>
        <button type="button" className={smallButton} onClick={() => setConfirmDelete(null)}>
          {t('printersCancel')}
        </button>
        <button type="button" className={primaryButton} onClick={runDelete}>
          {t('printersDelete')}
        </button>
      </div>
    ) : null;

  return (
    <>
      <div
        className={`fixed inset-0 bg-black/45 transition-opacity duration-150 z-40 ${
          open ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        onClick={onClose}
      />
      <aside
        aria-label={t('printersManageTitle')}
        aria-hidden={!open}
        className={`fixed top-0 right-0 bottom-0 w-full max-w-[400px] bg-[var(--panel)] border-l border-[var(--line)] shadow-[var(--shadow)] z-50 flex flex-col transition-transform duration-200 ${
          open ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        <div className="flex-none flex items-center justify-between px-4 py-3.5 border-b border-[var(--line)]">
          <h3 className="text-[15px] font-bold m-0">{t('printersManageTitle')}</h3>
          <button
            type="button"
            onClick={onClose}
            aria-label={t('printersClose')}
            className="w-7 h-7 rounded-md grid place-items-center text-[var(--ink-3)] hover:bg-[var(--panel-2)] cursor-pointer"
          >
            ✕
          </button>
        </div>

        {open && (
          <div className="flex-1 overflow-y-auto px-4 py-4 flex flex-col gap-4">
            {error && <div className="text-[12.5px] text-[var(--accent)] break-words">{t('filamentError')} {error}</div>}

            {printers.map((printer) => (
              <div key={printer.id} className="rounded-lg border border-[var(--line)] bg-[var(--panel-2)] p-3">
                <div className="flex items-center gap-2">
                  {renameRow(printer.id, 'printer') ?? (
                    <>
                      <span className="flex-1 text-[13.5px] font-bold truncate">{printer.name}</span>
                      <button
                        type="button"
                        className={smallButton}
                        onClick={() => setRenaming({ id: printer.id, kind: 'printer', value: printer.name })}
                      >
                        {t('printersRename')}
                      </button>
                      <button
                        type="button"
                        className={smallButton}
                        onClick={() => setConfirmDelete({ id: printer.id, kind: 'printer' })}
                      >
                        {t('printersDelete')}
                      </button>
                    </>
                  )}
                </div>
                {confirmRow(
                  printer.id,
                  'printer',
                  t('printersDeletePrinterConfirm')
                    .replace('{name}', printer.name)
                    .replace('{count}', String(spoolsIn(printer.units.map((u) => u.id)))),
                )}

                <div className="flex flex-col gap-1.5 mt-2.5">
                  {printer.units.map((unit) => (
                    <div
                      key={unit.id}
                      data-testid={`unit-row-${unit.id}`}
                      onMouseEnter={() => dragUnit && setDragOverUnit(unit.id)}
                      className={`rounded-md border bg-[var(--panel)] px-2 py-1.5 ${
                        dragOverUnit === unit.id ? 'border-[var(--accent)]' : 'border-[var(--line)]'
                      }`}
                    >
                      <div className="flex items-center gap-2">
                        <span
                          role="button"
                          aria-label={t('printersReorderHint')}
                          title={t('printersReorderHint')}
                          onMouseDown={(e) => {
                            e.preventDefault();
                            setDragUnit({ printerId: printer.id, unitId: unit.id });
                          }}
                          className="text-[var(--ink-3)] cursor-grab select-none px-0.5"
                        >
                          ⋮⋮
                        </span>
                        {renameRow(unit.id, 'unit') ?? (
                          <>
                            <span className="flex-1 min-w-0">
                              <span className="block text-[12.5px] font-semibold truncate">{unit.name}</span>
                              <span className="block text-[11px] text-[var(--ink-3)]">
                                {t(KIND_LABEL[unit.kind])} · {formatCount(t('printersUnitSlotCount'), unit.slotCount)}
                              </span>
                            </span>
                            {unit.kind === 'custom' && (
                              <Stepper
                                value={unit.slotCount}
                                onChange={(n) => changeCustomSlots(unit, n)}
                                label={t('printersSlotsLabel')}
                              />
                            )}
                            <button
                              type="button"
                              className={smallButton}
                              onClick={() => setRenaming({ id: unit.id, kind: 'unit', value: unit.name })}
                            >
                              {t('printersRename')}
                            </button>
                            <button
                              type="button"
                              className={smallButton}
                              aria-label={`${t('printersDelete')} ${unit.name}`}
                              onClick={() => setConfirmDelete({ id: unit.id, kind: 'unit' })}
                            >
                              ✕
                            </button>
                          </>
                        )}
                      </div>
                      {confirmRow(
                        unit.id,
                        'unit',
                        t('printersDeleteUnitConfirm')
                          .replace('{name}', unit.name)
                          .replace('{count}', String(spoolsIn([unit.id]))),
                      )}
                    </div>
                  ))}
                </div>

                {customFor?.printerId === printer.id ? (
                  <div className="flex items-center gap-1.5 mt-2">
                    <input
                      autoFocus
                      value={customFor.name}
                      onChange={(e) => setCustomFor({ ...customFor, name: e.target.value })}
                      placeholder={t('printersCustomUnitNamePlaceholder')}
                      className={`${fieldClass} flex-1 min-w-0`}
                    />
                    <Stepper
                      value={customFor.slots}
                      onChange={(slots) => setCustomFor({ ...customFor, slots })}
                      label={t('printersSlotsLabel')}
                    />
                    <button
                      type="button"
                      className={primaryButton}
                      disabled={!customFor.name.trim()}
                      onClick={() =>
                        actions
                          .addUnit(printer.id, 'custom', customFor.name.trim(), customFor.slots)
                          .then(() => setCustomFor(null), () => {})
                      }
                    >
                      {t('printersAddUnitButton')}
                    </button>
                  </div>
                ) : (
                  <div className="relative mt-2">
                    <button
                      type="button"
                      className={smallButton}
                      onClick={() => setAddMenuFor(addMenuFor === printer.id ? null : printer.id)}
                    >
                      + {t('printersAddUnitButton')} ▾
                    </button>
                    {addMenuFor === printer.id && (
                      <div
                        role="menu"
                        className="absolute left-0 top-full mt-1 z-10 min-w-[200px] rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] py-1"
                      >
                        {UNIT_TEMPLATES.map((template) => (
                          <button
                            key={template.kind}
                            type="button"
                            role="menuitem"
                            onClick={() => addTemplate(printer, template.kind)}
                            className="w-full flex justify-between gap-3 text-left px-2.5 py-1.5 text-[12px] hover:bg-[var(--panel-2)] cursor-pointer"
                          >
                            <span>{t(KIND_LABEL[template.kind])}</span>
                            {template.slotCount !== null && (
                              <span className="text-[var(--ink-3)]">{template.slotCount}</span>
                            )}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                )}
              </div>
            ))}

            <div className="flex items-center gap-1.5">
              <input
                value={newPrinter}
                onChange={(e) => setNewPrinter(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && submitNewPrinter()}
                placeholder={t('printersNewPrinterPlaceholder')}
                className={`${fieldClass} flex-1 min-w-0`}
              />
              <button type="button" className={primaryButton} disabled={!newPrinter.trim()} onClick={submitNewPrinter}>
                {t('printersAddPrinterButton')}
              </button>
            </div>
          </div>
        )}
      </aside>
    </>
  );
}
