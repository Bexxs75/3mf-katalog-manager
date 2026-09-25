import { useEffect, useMemo, useRef, useState } from 'react';
import type { MouseEvent as ReactMouseEvent } from 'react';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatStockG } from '../i18n/format';
import { filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { isInStorage, slotKey, spoolsBySlot } from '../lib/filamentSlots';
import { isValidColorHex } from '../lib/filamentColors';
import type { SpoolDropTarget } from '../hooks/useSpoolDragAndDrop';
import type { PrinterLinkState } from '../hooks/usePrinterLink';
import type { FilamentSpool, MaterialUnit, Printer } from '../types';
import { PrinterLinkStatus } from './PrinterLinkStatus';

interface Props {
  printers: Printer[];
  /** Alle Spulen (Lager und Faecher). */
  spools: FilamentSpool[];
  draggingSpoolId: string | null;
  dropTarget: SpoolDropTarget | null;
  onSlotMouseDown: (spoolId: string, unitId: string, slotIndex: number, event: ReactMouseEvent) => void;
  onEnterSlot: (unitId: string, slotIndex: number) => void;
  onLeaveSlot: (unitId: string, slotIndex: number) => void;
  onLoad: (spoolId: string, unitId: string, slotIndex: number) => void;
  onUnload: (spoolId: string) => void;
  onEditSpool: (spool: FilamentSpool) => void;
  onManage: () => void;
  printerLink?: PrinterLinkState;
}

const barClass: Record<string, string> = {
  ok: 'bg-[var(--good)]',
  low: 'bg-[var(--warn)]',
  empty: 'bg-[var(--crit)]',
};

interface OpenMenu {
  unitId: string;
  slotIndex: number;
}

/** Feste Spalte rechts im Filament-Lager: Drucker, Einheiten, Faecher. */
export function PrinterColumn({
  printers,
  spools,
  draggingSpoolId,
  dropTarget,
  onSlotMouseDown,
  onEnterSlot,
  onLeaveSlot,
  onLoad,
  onUnload,
  onEditSpool,
  onManage,
  printerLink,
}: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [menu, setMenu] = useState<OpenMenu | null>(null);
  const bySlot = useMemo(() => spoolsBySlot(spools), [spools]);
  const storage = useMemo(() => spools.filter(isInStorage), [spools]);

  const isTarget = (unitId: string, slotIndex: number) =>
    draggingSpoolId !== null &&
    dropTarget?.kind === 'slot' &&
    dropTarget.unitId === unitId &&
    dropTarget.slotIndex === slotIndex;

  const renderUnit = (unit: MaterialUnit) => {
    // Verteidigung in der Tiefe (finaler Review 2026-09-23, Finding 1): das
    // Backend lehnt eine `slotCount` ausserhalb 1..16 bereits beim Import
    // eines Katalog-Backups ab (`validate_printer_invariants`), aber diese
    // Obergrenze hier stellt sicher, dass eine Ansicht auch dann nie haengt
    // (`Array.from({length: unit.slotCount})`), wenn irgendein anderer,
    // noch unbekannter Pfad einmal einen zu grossen Wert liefert.
    const slotCount = Math.min(unit.slotCount, 16);
    const used = Array.from({ length: slotCount }, (_, i) => bySlot.get(slotKey(unit.id, i))).filter(Boolean).length;
    return (
      <div key={unit.id} className="rounded-md border border-[var(--line)] bg-[var(--panel-2)] p-2">
        <div className="flex items-center justify-between text-[11px] font-semibold text-[var(--ink-2)] mb-1.5">
          <span className="truncate">{unit.name}</span>
          <span className="font-mono-ui text-[var(--ink-3)]">
            {used}/{slotCount}
          </span>
        </div>
        <div className="flex flex-col gap-1">
          {Array.from({ length: slotCount }, (_, slotIndex) => {
            const spool = bySlot.get(slotKey(unit.id, slotIndex)) ?? null;
            const target = isTarget(unit.id, slotIndex);
            const menuOpen = menu?.unitId === unit.id && menu.slotIndex === slotIndex;
            return (
              <div key={slotIndex} className="relative">
                <button
                  type="button"
                  data-testid={`slot-${unit.id}-${slotIndex}`}
                  onMouseDown={(e) => spool && onSlotMouseDown(spool.id, unit.id, slotIndex, e)}
                  onMouseEnter={() => onEnterSlot(unit.id, slotIndex)}
                  onMouseLeave={() => onLeaveSlot(unit.id, slotIndex)}
                  onClick={() => setMenu(menuOpen ? null : { unitId: unit.id, slotIndex })}
                  className={`w-full flex items-center gap-2 px-2 py-1.5 rounded-[5px] border text-left text-[11.5px] cursor-pointer ${
                    target
                      ? 'border-2 border-dashed border-[var(--accent)] bg-[var(--accent-soft)]'
                      : spool
                        ? 'border-[var(--line)] bg-[var(--panel)] hover:border-[var(--line-strong)]'
                        : 'border-dashed border-[var(--line-strong)] text-[var(--ink-3)] hover:border-[var(--accent)]'
                  }`}
                >
                  <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)] w-3 flex-none">{slotIndex + 1}</span>
                  {spool ? (
                    <>
                      <span
                        className="w-3.5 h-3.5 rounded-[3px] border border-[var(--line-strong)] flex-none"
                        style={spool.colorHex && isValidColorHex(spool.colorHex) ? { backgroundColor: spool.colorHex } : undefined}
                        aria-hidden
                      />
                      <span className="min-w-0 flex-1">
                        <span className="block truncate font-semibold text-[var(--ink)]">
                          {[spool.material, spool.color].filter(Boolean).join(' · ')}
                        </span>
                        <span className="flex items-center gap-1.5">
                          <span className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
                            {formatStockG(spool.remainingWeightG, language)}
                          </span>
                          <span className="flex-1 h-1 rounded-full bg-[var(--plate)] overflow-hidden">
                            <span
                              className={`block h-full rounded-full ${barClass[filamentStockStatus(spool)]}`}
                              style={{ width: `${filamentStockPercent(spool)}%` }}
                            />
                          </span>
                        </span>
                      </span>
                    </>
                  ) : (
                    <span className="flex-1">{target ? t('printersDropHere') : t('printersSlotEmpty')}</span>
                  )}
                </button>
                {menuOpen && (
                  <SlotMenu
                    spool={spool}
                    storage={storage}
                    onClose={() => setMenu(null)}
                    onLoad={(spoolId) => onLoad(spoolId, unit.id, slotIndex)}
                    onUnload={onUnload}
                    onEdit={onEditSpool}
                  />
                )}
              </div>
            );
          })}
        </div>
      </div>
    );
  };

  return (
    <aside className="w-full lg:w-[260px] flex-none flex flex-col gap-3" aria-label={t('printersColumnTitle')}>
      <div className="flex items-center justify-between">
        <span className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)]">{t('printersColumnTitle')}</span>
        {printers.length > 0 && (
          <button
            type="button"
            onClick={onManage}
            className="text-[11.5px] text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
          >
            {t('printersManageButton')}
          </button>
        )}
      </div>

      {printers.length === 0 ? (
        <div className="rounded-lg border border-dashed border-[var(--line-strong)] p-3 text-[12px] text-[var(--ink-3)] flex flex-col gap-2">
          <span>{t('printersEmptyHint')}</span>
          <button
            type="button"
            onClick={onManage}
            className="self-start h-8 px-3 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-bold cursor-pointer"
          >
            {t('printersAddFirstButton')}
          </button>
        </div>
      ) : (
        printers.map((printer) => (
          <div key={printer.id} className="flex flex-col gap-2">
            <div className="text-[12.5px] font-bold">{printer.name}</div>
            {printerLink && <PrinterLinkStatus printerId={printer.id} link={printerLink} />}
            {printer.units.map(renderUnit)}
            {printer.units.length === 0 && (
              <button
                type="button"
                onClick={onManage}
                className="self-start text-left text-[11.5px] text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
              >
                {t('printersNoUnitsHint')}
              </button>
            )}
          </div>
        ))
      )}
    </aside>
  );
}

interface SlotMenuProps {
  spool: FilamentSpool | null;
  storage: FilamentSpool[];
  onClose: () => void;
  onLoad: (spoolId: string) => void;
  onUnload: (spoolId: string) => void;
  onEdit: (spool: FilamentSpool) => void;
}

function SlotMenu({ spool, storage, onClose, onLoad, onUnload, onEdit }: SlotMenuProps) {
  const t = useT();
  const [query, setQuery] = useState('');
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const handleDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    // Erst nach dem aktuellen Klick registrieren, sonst schliesst der Klick,
    // der das Menue oeffnet, es sofort wieder.
    const timer = setTimeout(() => document.addEventListener('mousedown', handleDown));
    document.addEventListener('keydown', handleKey);
    return () => {
      clearTimeout(timer);
      document.removeEventListener('mousedown', handleDown);
      document.removeEventListener('keydown', handleKey);
    };
  }, [onClose]);

  const needle = query.trim().toLowerCase();
  const candidates = storage.filter(
    (s) => !needle || [s.material, s.manufacturer, s.color, s.location].some((v) => v?.toLowerCase().includes(needle)),
  );

  const action = (label: string, run: () => void) => (
    <button
      type="button"
      onClick={() => {
        run();
        onClose();
      }}
      className="w-full text-left px-2.5 py-1.5 text-[12px] hover:bg-[var(--panel-2)] cursor-pointer"
    >
      {label}
    </button>
  );

  return (
    <div
      ref={ref}
      role="menu"
      className="absolute left-0 right-0 top-full mt-1 z-30 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] shadow-[var(--shadow)] py-1"
    >
      {spool && (
        <>
          {action(t('printersSlotMenuUnload'), () => onUnload(spool.id))}
          {action(t('printersSlotMenuEdit'), () => onEdit(spool))}
          <div className="border-t border-[var(--line)] my-1" />
        </>
      )}
      <div className="px-2.5 pt-1 pb-1.5 text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)]">
        {t('printersSlotMenuLoad')}
      </div>
      <div className="px-2 pb-1.5">
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t('printersSlotMenuSearch')}
          className="w-full h-7 px-2 rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12px] outline-0 focus:border-[var(--accent)]"
        />
      </div>
      <div className="max-h-48 overflow-y-auto">
        {candidates.length === 0 ? (
          <div className="px-2.5 py-1.5 text-[11.5px] text-[var(--ink-3)]">{t('printersSlotMenuNoSpools')}</div>
        ) : (
          candidates.map((candidate) => (
            <button
              key={candidate.id}
              type="button"
              onClick={() => {
                onLoad(candidate.id);
                onClose();
              }}
              className="w-full flex items-center gap-2 text-left px-2.5 py-1.5 text-[12px] hover:bg-[var(--panel-2)] cursor-pointer"
            >
              <span
                className="w-3 h-3 rounded-full border border-[var(--line-strong)] flex-none"
                style={candidate.colorHex && isValidColorHex(candidate.colorHex) ? { backgroundColor: candidate.colorHex } : undefined}
                aria-hidden
              />
              <span className="flex-1 truncate">{[candidate.material, candidate.color].filter(Boolean).join(' · ')}</span>
              <span className="text-[10.5px] text-[var(--ink-3)] truncate max-w-[80px]">{candidate.location}</span>
            </button>
          ))
        )}
      </div>
    </div>
  );
}
