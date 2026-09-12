import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import type { FilamentSpool } from '../types';
import { FILAMENT_MATERIALS, FILAMENT_MANUFACTURERS } from '../lib/filamentCatalog';
import { filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { AutocompleteInput } from './AutocompleteInput';

interface Props {
  open: boolean;
  editing: FilamentSpool | null;
  knownLocations: string[];
  onClose: () => void;
  onSaved: () => void;
}

interface FormState {
  material: string;
  manufacturer: string;
  color: string;
  location: string;
  diameterMm: string;
  originalWeightG: string;
  remainingWeightG: string;
  price: string;
  imagePng: string | null;
  quantity: string;
}

const EMPTY_FORM: FormState = {
  material: '',
  manufacturer: '',
  color: '',
  location: '',
  diameterMm: '1.75',
  originalWeightG: '1000',
  remainingWeightG: '1000',
  price: '',
  imagePng: null,
  quantity: '1',
};

const fieldClass =
  'w-full h-9 px-2.5 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] outline-0 text-[13px] focus:border-[var(--accent)]';

function toForm(spool: FilamentSpool): FormState {
  return {
    material: spool.material,
    manufacturer: spool.manufacturer ?? '',
    color: spool.color ?? '',
    location: spool.location ?? '',
    diameterMm: String(spool.diameterMm),
    originalWeightG: String(spool.originalWeightG),
    remainingWeightG: String(spool.remainingWeightG),
    price: spool.price === null ? '' : String(spool.price),
    imagePng: spool.imagePng,
    quantity: '1',
  };
}

export function FilamentSpoolForm({ open, editing, knownLocations, onClose, onSaved }: Props) {
  const t = useT();
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      setForm(editing ? toForm(editing) : EMPTY_FORM);
      setError(null);
    }
  }, [open, editing]);

  const handlePickImage = () => {
    invoke<string | null>('pick_and_read_image')
      .then((base64) => {
        if (base64 === null) return;
        setForm((prev) => ({ ...prev, imagePng: base64 }));
      })
      .catch((e) => setError(String(e)));
  };

  const submit = async () => {
    if (!form.material.trim()) return;
    const payload: FilamentSpool = {
      id: editing?.id ?? '',
      material: form.material.trim(),
      manufacturer: form.manufacturer.trim() || null,
      color: form.color.trim() || null,
      location: form.location.trim() || null,
      diameterMm: parseFloat(form.diameterMm) || 0,
      originalWeightG: parseInt(form.originalWeightG, 10) || 0,
      remainingWeightG: parseInt(form.remainingWeightG, 10) || 0,
      price: form.price.trim() === '' ? null : parseFloat(form.price),
      imagePng: form.imagePng,
    };
    try {
      if (editing) {
        await invoke('update_filament_spool', { spool: payload });
      } else {
        // Jede Spule bekommt einen eigenen Datensatz (eigene id), auch bei
        // identischen Werten - der Restbestand wird pro physischer Spule
        // unabhaengig verfolgt (z.B. Schwarz PLA nutzt sich unterschiedlich
        // schnell ab, je nachdem welche Spule gerade im Drucker steckt).
        const count = Math.max(1, parseInt(form.quantity, 10) || 1);
        for (let i = 0; i < count; i++) {
          await invoke('add_filament_spool', { spool: payload });
        }
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(String(e));
    }
  };

  const original = parseFloat(form.originalWeightG) || 0;
  const remaining = parseFloat(form.remainingWeightG) || 0;
  const previewStatus = filamentStockStatus({ originalWeightG: original, remainingWeightG: remaining });
  const previewPct = filamentStockPercent({ originalWeightG: original, remainingWeightG: remaining });
  const barColor =
    previewStatus === 'empty' ? 'var(--crit)' : previewStatus === 'low' ? 'var(--warn)' : 'var(--good)';

  return (
    <>
      <div
        className={`fixed inset-0 bg-black/45 transition-opacity duration-150 z-40 ${
          open ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        onClick={onClose}
      />
      <aside
        className={`fixed top-0 right-0 bottom-0 w-full max-w-[380px] bg-[var(--panel)] border-l border-[var(--line)] shadow-[var(--shadow)] z-50 flex flex-col transition-transform duration-200 ${
          open ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        <div className="flex-none flex items-center justify-between px-4 py-3.5 border-b border-[var(--line)]">
          <h3 className="text-[15px] font-bold m-0">
            {editing ? t('filamentSaveButton') : t('filamentOpenAddPanelButton')}
          </h3>
          <button
            onClick={onClose}
            aria-label={t('cancel')}
            className="w-7 h-7 rounded-md grid place-items-center text-[var(--ink-3)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)] cursor-pointer"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-4 flex flex-col gap-5">
          {error && <div className="text-[12.5px] text-[var(--accent)] break-words">{t('filamentError')} {error}</div>}

          <div>
            <p className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-2">
              {t('filamentSectionImage')}
            </p>
            <button
              type="button"
              onClick={handlePickImage}
              className="w-full flex flex-col items-center gap-1.5 px-4 py-4 rounded-lg border-[1.5px] border-dashed border-[var(--line-strong)] text-[var(--ink-3)] text-[12px] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {form.imagePng ? (
                <img
                  src={`data:image/png;base64,${form.imagePng}`}
                  className="w-14 h-14 rounded-md object-cover border border-[var(--line)]"
                />
              ) : (
                <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8">
                  <rect x="3" y="3" width="18" height="18" rx="3" />
                  <circle cx="8.5" cy="8.5" r="1.5" />
                  <path d="m21 15-5-5L5 21" />
                </svg>
              )}
              {t('filamentImageDropHint')}
            </button>
          </div>

          <div>
            <p className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-2">
              {t('filamentSectionIdentification')}
            </p>
            <div className="flex flex-col gap-2.5">
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentMaterialLabel')}</label>
                <AutocompleteInput
                  value={form.material}
                  onChange={(v) => setForm((f) => ({ ...f, material: v }))}
                  options={FILAMENT_MATERIALS}
                  placeholder={t('filamentMaterialLabel')}
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentManufacturerLabel')}</label>
                <AutocompleteInput
                  value={form.manufacturer}
                  onChange={(v) => setForm((f) => ({ ...f, manufacturer: v }))}
                  options={FILAMENT_MANUFACTURERS}
                  placeholder={t('filamentManufacturerLabel')}
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentColorLabel')}</label>
                <input
                  value={form.color}
                  onChange={(e) => setForm((f) => ({ ...f, color: e.target.value }))}
                  placeholder={t('filamentColorLabel')}
                  className={fieldClass}
                />
              </div>

              {!editing && (
                <div>
                  <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentQuantityLabel')}</label>
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={() => setForm((f) => ({ ...f, quantity: String(Math.max(1, (parseInt(f.quantity, 10) || 1) - 1)) }))}
                      className="w-9 h-9 flex-none rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[15px] font-bold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      −
                    </button>
                    <input
                      type="number"
                      min="1"
                      value={form.quantity}
                      onChange={(e) => setForm((f) => ({ ...f, quantity: e.target.value }))}
                      className={`${fieldClass} text-center [appearance:textfield] [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none`}
                    />
                    <button
                      type="button"
                      onClick={() => setForm((f) => ({ ...f, quantity: String((parseInt(f.quantity, 10) || 1) + 1) }))}
                      className="w-9 h-9 flex-none rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[15px] font-bold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      +
                    </button>
                  </div>
                  <p className="text-[10.5px] text-[var(--ink-3)] mt-1 leading-snug">{t('filamentQuantityHint')}</p>
                </div>
              )}
            </div>
          </div>

          <div>
            <p className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-2">
              {t('filamentSectionStorage')}
            </p>
            <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentLocationLabel')}</label>
            <AutocompleteInput
              value={form.location}
              onChange={(v) => setForm((f) => ({ ...f, location: v }))}
              options={knownLocations}
              placeholder={t('filamentLocationLabel')}
              className={fieldClass}
            />
          </div>

          <div>
            <p className="text-[10px] uppercase tracking-wider font-bold text-[var(--ink-3)] mb-2">
              {t('filamentSectionStock')}
            </p>
            <div className="grid grid-cols-2 gap-2.5">
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentDiameterLabel')}</label>
                <input
                  type="number"
                  step="0.01"
                  value={form.diameterMm}
                  onChange={(e) => setForm((f) => ({ ...f, diameterMm: e.target.value }))}
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentPriceLabel')}</label>
                <input
                  type="number"
                  step="0.01"
                  value={form.price}
                  onChange={(e) => setForm((f) => ({ ...f, price: e.target.value }))}
                  placeholder="24.90"
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentOriginalWeightLabel')}</label>
                <input
                  type="number"
                  value={form.originalWeightG}
                  onChange={(e) => setForm((f) => ({ ...f, originalWeightG: e.target.value }))}
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-[11.5px] font-semibold text-[var(--ink-2)] mb-1">{t('filamentRemainingWeightLabel')}</label>
                <input
                  type="number"
                  value={form.remainingWeightG}
                  onChange={(e) => setForm((f) => ({ ...f, remainingWeightG: e.target.value }))}
                  className={fieldClass}
                />
              </div>
            </div>
            <div className="flex flex-col gap-1.5 mt-3">
              <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
                <div
                  className="h-full rounded-full transition-[width]"
                  style={{ width: `${previewPct}%`, background: barColor }}
                />
              </div>
              <div className="font-mono-ui text-[13px] font-bold" style={{ color: barColor }}>
                {previewPct}&nbsp;%
              </div>
            </div>
          </div>
        </div>

        <div className="flex-none px-4 py-3.5 border-t border-[var(--line)] bg-[var(--panel-2)]">
          <button
            onClick={submit}
            disabled={!form.material.trim()}
            className="w-full h-10 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[13.5px] font-bold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {editing
              ? t('filamentSaveButton')
              : (parseInt(form.quantity, 10) || 1) > 1
                ? `${t('filamentAddButton')} (${Math.max(1, parseInt(form.quantity, 10) || 1)}×)`
                : t('filamentAddButton')}
          </button>
        </div>
      </aside>
    </>
  );
}
