import { canEditMetadata, useCanEditMetadata } from '../lib/webSession';
import { readPrintLogPhoto } from '../lib/printLogPhoto';
import { invalidateSpoolImage } from '../lib/spoolImages';
import { SpoolImage } from './SpoolImage';
import { Icon } from './Icon';
import { useModalDialog } from '../hooks/useModalDialog';
import { useCallback, useEffect, useState, useRef, useId } from 'react';
import { invoke } from '../lib/transport';
import { useT } from '../i18n/LanguageContext';
import { parseDecimalInput } from '../lib/decimalInput';
import type { FilamentSpool, SpoolKind } from '../types';
import { FILAMENT_MATERIALS, RESIN_MATERIALS, manufacturerSuggestions } from '../lib/filamentCatalog';
import { filamentStockPercent, filamentStockStatus } from '../lib/filamentStatus';
import { isValidColorHex } from '../lib/filamentColors';
import { suggestSpoolWeight } from '../lib/spoolWeightSuggestion';
import { useImageDropZone } from '../hooks/useImageDropZone';
import { readDroppedImage } from '../lib/api/filament';
import type { ImageDropRejection } from '../lib/imageDrop';
import { expectedError, toAppError, type AppError } from '../lib/errors';
import { AutocompleteInput } from './AutocompleteInput';
import { ColorPicker } from './ColorPicker';
import { ErrorText } from '../diagnostics/ErrorText';

interface Props {
  open: boolean;
  editing: FilamentSpool | null;
  knownLocations: string[];
  knownManufacturers?: string[];
  /** All spools: the start weight of a new spool comes from the newest matching one. */
  knownSpools?: FilamentSpool[];
  onClose: () => void;
  onSaved: () => void;
  /** Kind for new entries (current view in the inventory). */
  defaultKind?: SpoolKind;
}

interface FormState {
  kind: SpoolKind;
  material: string;
  manufacturer: string;
  color: string;
  colorHex: string | null;
  /** Texts of the colors 2 to 4 as typed; the first color is `colorHex`. */
  extraColors: string[];
  location: string;
  diameterMm: string;
  originalWeightG: string;
  remainingWeightG: string;
  price: string;
  imagePng: string | null;
  quantity: string;
}

const EMPTY_FORM: FormState = {
  kind: 'filament',
  material: '',
  manufacturer: '',
  color: '',
  colorHex: null,
  extraColors: [],
  location: '',
  diameterMm: '1.75',
  originalWeightG: '1000',
  remainingWeightG: '1000',
  price: '',
  imagePng: null,
  quantity: '1',
};

/** Most colors one spool can carry (the backend checks the same). */
const MAX_COLORS = 4;
const NO_SPOOLS: FilamentSpool[] = [];

/** `1a1a1a` and `#1A1A1A` both become `#1a1a1a`; whatever else stays as typed. */
function normalizeHex(text: string): string {
  const trimmed = text.trim();
  return (trimmed.startsWith('#') ? trimmed : `#${trimmed}`).toLowerCase();
}

type NumericField = 'diameterMm' | 'price' | 'originalWeightG' | 'remainingWeightG';

const fieldClass =
  'w-full h-9 px-2.5 rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] outline-0 text-body focus:border-[var(--accent)]';

function toForm(spool: FilamentSpool): FormState {
  return {
    kind: spool.kind,
    material: spool.material,
    manufacturer: spool.manufacturer ?? '',
    color: spool.color ?? '',
    colorHex: spool.colorHexes[0] ?? spool.colorHex,
    extraColors: spool.colorHexes.slice(1),
    // If the spool sits in a slot, the field edits its home location.
    location: (spool.unitId !== null ? spool.homeLocation : spool.location) ?? '',
    diameterMm: String(spool.diameterMm),
    originalWeightG: String(spool.originalWeightG),
    remainingWeightG: String(spool.remainingWeightG),
    price: spool.price === null ? '' : String(spool.price),
    imagePng: spool.imagePng ?? null,
    quantity: '1',
  };
}

export function FilamentSpoolForm({ open, editing, knownLocations, knownManufacturers = [], knownSpools = NO_SPOOLS, onClose, onSaved, defaultKind = 'filament' }: Props) {
  const t = useT();
  const canEdit = useCanEditMetadata();
  const web = import.meta.env.VITE_TARGET === 'web';
  const [imageChanged, setImageChanged] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [readingImage, setReadingImage] = useState(false);
  const imageReadEpoch = useRef(0);
  const [partial, setPartial] = useState('');
  const fileInput = useRef<HTMLInputElement>(null);
  const dialogRef = useModalDialog<HTMLElement>({ open, onClose });
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const fieldId = useId();
  const numericRefs = useRef<Partial<Record<NumericField, HTMLInputElement | null>>>({});
  const [attempted, setAttempted] = useState(false);
  // A weight the user typed is never replaced by the suggestion for the maker and material.
  const [weightTouched, setWeightTouched] = useState({ original: false, remaining: false });
  const [pendingFocus, setPendingFocus] = useState<string | null>(null);
  const diameter = parseDecimalInput(form.diameterMm);
  const price = form.price.trim() === '' ? null : parseDecimalInput(form.price);
  const original = parseDecimalInput(form.originalWeightG);
  const remaining = parseDecimalInput(form.remainingWeightG);
  // Validate the stored precision too, so a positive entry cannot become a zero amount.
  const roundedOriginal = Math.round(original * 10) / 10;
  const invalid: Record<NumericField, boolean> = {
    diameterMm: form.kind !== 'resin' && (!Number.isFinite(diameter) || diameter <= 0 || diameter > 5),
    price: price !== null && (!Number.isFinite(price) || price < 0),
    originalWeightG: !Number.isFinite(original) || original <= 0 || !Number.isFinite(roundedOriginal) || roundedOriginal <= 0,
    remainingWeightG: !Number.isFinite(remaining) || remaining < 0 || remaining > original,
  };
  const extraInvalid = form.extraColors.map((text) => text.trim() !== '' && !isValidColorHex(normalizeHex(text)));
  const messages = {
    diameterMm: t('filamentInvalidDiameter'), price: t('filamentInvalidPrice'),
    originalWeightG: t('filamentInvalidOriginal'), remainingWeightG: t('filamentInvalidRemaining'),
  };
  const numericProps = (key: NumericField) => ({
    id: `${fieldId}-${key}`,
    ref: (node: HTMLInputElement | null) => { numericRefs.current[key] = node; },
    'aria-invalid': attempted && invalid[key] ? true : undefined,
    'aria-describedby': attempted && invalid[key] ? `${fieldId}-${key}-error` : undefined,
    className: `${fieldClass}${attempted && invalid[key] ? ' !border-[var(--crit)]' : ''}`,
  });
  const fieldError = (key: NumericField) => attempted && invalid[key]
    ? <p id={`${fieldId}-${key}-error`} role="alert" className="mt-1 text-caption text-[var(--crit)]">{messages[key]}</p>
    : null;
  const [error, setError] = useState<AppError | null>(null);
  // Rejected image (too large, not an image, several files): shown right at
  // the image field and keeps the form open until another image arrives or
  // the hint is closed - otherwise it would silently save without an image.
  const [imageError, setImageError] = useState<AppError | null>(null);
  const imageErrorRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (open) {
      setForm(editing ? toForm(editing) : { ...EMPTY_FORM, kind: defaultKind });
      setWeightTouched({ original: false, remaining: false });
      setError(null);
      imageReadEpoch.current++;
      setReadingImage(false);
      setImageChanged(false);
      setPartial('');
      setAttempted(false);
      setImageError(null);
    }
  }, [open, editing, defaultKind]);

  // New spool: start weights follow the maker and material until the user types one.
  const weightSuggestion = suggestSpoolWeight(knownSpools, form.kind, form.manufacturer, form.material);
  useEffect(() => {
    if (!open || editing) return;
    setForm((prev) => {
      const weight = String(suggestSpoolWeight(knownSpools, prev.kind, prev.manufacturer, prev.material).weight);
      const next = {
        ...prev,
        originalWeightG: weightTouched.original ? prev.originalWeightG : weight,
        remainingWeightG: weightTouched.remaining ? prev.remainingWeightG : weight,
      };
      return next.originalWeightG === prev.originalWeightG && next.remainingWeightG === prev.remainingWeightG ? prev : next;
    });
  }, [open, editing, knownSpools, form.kind, form.manufacturer, form.material, weightTouched]);

  useEffect(() => {
    if (pendingFocus === null) return;
    document.getElementById(pendingFocus)?.focus();
    setPendingFocus(null);
  }, [pendingFocus, form.extraColors.length]);

  const addColor = () => {
    const id = `${fieldId}-extra-${form.extraColors.length}`;
    setForm((f) => (f.extraColors.length + 1 >= MAX_COLORS ? f : { ...f, extraColors: [...f.extraColors, ''] }));
    setPendingFocus(id);
  };
  const removeColor = (index: number) => {
    setForm((f) => ({ ...f, extraColors: f.extraColors.filter((_, i) => i !== index) }));
    setPendingFocus(`${fieldId}-add-color`);
  };

  const handlePickImage = () => {
    if (!canEditMetadata()) return;
    if (web) { fileInput.current?.click(); return; }
    invoke<string | null>('pick_and_read_image')
      .then((base64) => {
        if (base64 === null) return;
        setImageError(null);
        setForm((prev) => ({ ...prev, imagePng: base64 }));
      })
      .catch((e) => setImageError(toAppError(e)));
  };

  const handleDroppedImage = useCallback((path: string) => {
    readDroppedImage(path)
      .then((base64) => {
        setImageError(null);
        setForm((prev) => ({ ...prev, imagePng: base64 }));
      })
      .catch((e) => setImageError(toAppError(e)));
  }, []);

  const handleRejectedDrop = useCallback(
    (reason: ImageDropRejection) =>
      setImageError(expectedError(reason === 'multiple' ? t('filamentImageDropMultiple') : t('filamentImageDropNotImage'))),
    [t],
  );

  const imageDrop = useImageDropZone<HTMLButtonElement>({
    enabled: open && canEdit && !web,
    onImage: handleDroppedImage,
    onReject: handleRejectedDrop,
  });

  const submit = async () => {
    if (!canEditMetadata() || busyRef.current || readingImage || !form.material.trim()) return;
    setAttempted(true);
    const firstInvalid = (Object.keys(invalid) as NumericField[]).find(key => invalid[key]);
    if (firstInvalid) {
      numericRefs.current[firstInvalid]?.focus();
      return;
    }
    const badColor = extraInvalid.indexOf(true);
    if (badColor >= 0) {
      document.getElementById(`${fieldId}-extra-${badColor}`)?.focus();
      return;
    }
    if (imageError) {
      imageErrorRef.current?.scrollIntoView?.({ block: 'nearest', behavior: 'smooth' });
      return;
    }
    // One color stays single-colored (empty list); two or more become the list. The
    // backend keeps `colorHex` equal to the first entry.
    const listed = [form.colorHex, ...form.extraColors.map((text) => (text.trim() === '' ? null : normalizeHex(text)))]
      .filter((hex): hex is string => hex !== null && isValidColorHex(hex));
    const colorHexes = listed.length > 1 ? listed : [];
    const payload: Omit<FilamentSpool, 'imagePng'> & { imagePng?: string | null } = {
      id: editing?.id ?? '',
      material: form.material.trim(),
      manufacturer: form.manufacturer.trim() || null,
      color: form.color.trim() || null,
      location: form.location.trim() || null,
      diameterMm: form.kind === 'resin' && !Number.isFinite(diameter) ? 0 : diameter,
      originalWeightG: roundedOriginal,
      remainingWeightG: Math.round(remaining * 10) / 10,
      price,
      ...(!web || imageChanged ? { imagePng: form.imagePng } : {}),
      // A lone remaining extra color (first one set to "none") becomes the single color.
      colorHex: colorHexes[0] ?? listed[0] ?? form.colorHex,
      colorHexes,
      // Only passed through - the slot is changed exclusively by load/unload_spool.
      homeLocation: editing?.homeLocation ?? null,
      unitId: editing?.unitId ?? null,
      slotIndex: editing?.slotIndex ?? null,
      kind: form.kind,
    };
    if (web) { busyRef.current = true; setBusy(true); }
    let created = 0;
    const count = Math.max(1, parseInt(form.quantity, 10) || 1);
    try {
      if (editing) {
        await invoke('update_filament_spool', { spool: payload });
        if (web && imageChanged) invalidateSpoolImage(editing.id);
      } else {
        // Every spool gets its own record, even with identical values: the
        // remaining weight is tracked per physical spool.
        for (let i = 0; i < count; i++) {
          await invoke('add_filament_spool', { spool: payload });
          created++;
        }
      }
      onSaved();
      onClose();
    } catch (e) {
      if (web && created) {
        setForm(f => ({ ...f, quantity: String(count - created) }));
        setPartial(t('webManagerPartial').replace('{done}', String(created)).replace('{remaining}', String(count - created)));
        onSaved();
      }
      setError(toAppError(e));
    } finally { busyRef.current = false; setBusy(false); }
  };

  const resin = form.kind === 'resin';

  const previewStock = {
    originalWeightG: Number.isFinite(original) ? original : 0,
    remainingWeightG: Number.isFinite(remaining) ? remaining : 0,
  };
  const previewStatus = filamentStockStatus(previewStock);
  const previewPct = filamentStockPercent(previewStock);
  const barColor =
    previewStatus === 'empty' ? 'var(--crit)' : previewStatus === 'low' ? 'var(--warn)' : 'var(--good)';

  return (
    <>
      <div
        data-modal-backdrop
        className={`fixed inset-0 bg-black/45 transition-opacity duration-150 z-40 ${
          open ? 'opacity-100' : 'opacity-0 pointer-events-none'
        }`}
        onClick={onClose}
      />
      <aside
        ref={dialogRef}
        role={open ? "dialog" : undefined}
        aria-modal={open ? true : undefined}
        tabIndex={-1}
        aria-label={t(editing ? (resin ? 'resinEditAria' : 'filamentEditAria') : (resin ? 'resinOpenAddPanelButton' : 'filamentOpenAddPanelButton'))}
        aria-hidden={!open}
        // `inert` (not just aria-hidden) so the always-mounted, off-canvas panel's
        // controls are also unreachable by keyboard/Tab while closed (WCAG 4.1.2).
        inert={!open}
        className={`fixed top-0 right-0 bottom-0 w-full max-w-[380px] bg-[var(--panel)] border-l border-[var(--line)] shadow-[var(--shadow)] z-50 flex flex-col transition-transform duration-200 ${
          open ? 'translate-x-0' : 'translate-x-full'
        }`}
      >
        <div className="flex-none flex items-center justify-between px-4 py-3.5 border-b border-[var(--line)]">
          <h3 className="text-title font-bold m-0">
            {editing
              ? resin
                ? t('resinEditAria')
                : t('filamentEditAria')
              : resin
                ? t('resinOpenAddPanelButton')
                : t('filamentOpenAddPanelButton')}
          </h3>
          <button
            onClick={onClose}
            aria-label={t('cancel')}
            className="w-7 h-7 rounded-md grid place-items-center text-[var(--ink-3)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)] cursor-pointer"
          >
            <Icon name="close" size={14} />
          </button>
        </div>

        <div className="flex-1 scrollbar-stable overflow-y-scroll px-4 py-4 flex flex-col gap-5">
          {partial && <p role="status">{partial}</p>}
          {error && (
            <div role="alert" className="text-small text-[var(--accent)] break-words">
              {t('filamentError')} <ErrorText error={error} />
            </div>
          )}

          <div>
            <p className="text-compact-meta ui-label text-[var(--ink-3)] mb-2">
              {t('filamentSectionImage')}
            </p>
            {web && <input ref={fileInput} type="file" accept="image/*" className="hidden" aria-label={t('filamentSectionImage')} onChange={async e => {
              const file = e.target.files?.[0]; e.target.value = '';
              if (!file || !canEditMetadata()) return;
              const epoch = ++imageReadEpoch.current;
              setReadingImage(true);
              try {
                const imagePng = await readPrintLogPhoto(file);
                if (epoch !== imageReadEpoch.current) return;
                setForm(f => ({ ...f, imagePng })); setImageChanged(true); setImageError(null);
              } catch (error) {
                if (epoch === imageReadEpoch.current) setImageError(expectedError(t(error instanceof Error && error.message === 'webPhotoTooLarge' ? 'webPhotoTooLarge' : 'webPhotoFormat')));
              } finally { if (epoch === imageReadEpoch.current) setReadingImage(false); }
            }} />}
            {web && (form.imagePng || (!imageChanged && editing?.hasImage)) && <button type="button" disabled={!canEdit || busy || readingImage} onClick={() => { setImageChanged(true); setForm(f => ({...f, imagePng: null})); setImageError(null); }}>{t('webManagerRemoveImage')}</button>}
            <button
              disabled={!canEdit || busy || readingImage}
              type="button"
              ref={imageDrop.zoneRef}
              onClick={handlePickImage}
              data-drop-over={imageDrop.over ? 'true' : undefined}
              className={`w-full flex flex-col items-center gap-1.5 px-4 py-4 rounded-lg border-[1.5px] border-dashed text-small cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] ${
                imageDrop.over
                  ? 'border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-soft)]'
                  : 'border-[var(--line-strong)] text-[var(--ink-3)]'
              }`}
            >
              {web && !imageChanged && editing?.hasImage ? <SpoolImage spool={editing} className="w-14 h-14 rounded-md object-cover" fallback={<Icon name="spool" />} /> : form.imagePng ? (
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
              {t(web ? 'webManagerChooseImage' : 'filamentImageDropHint')}
            </button>
            {imageError && (
              <div
                ref={imageErrorRef}
                role="alert"
                className="mt-2 flex items-start gap-2 px-3 py-2 rounded-md border border-[var(--accent)] bg-[var(--accent-soft)] text-small text-[var(--ink)]"
              >
                <div className="flex-1 min-w-0 break-words">
                  <p className="font-semibold"><ErrorText error={imageError} /></p>
                  <p className="text-caption text-[var(--ink-2)] mt-0.5">{t('filamentImageErrorHint')}</p>
                </div>
                <button
                  type="button"
                  onClick={() => setImageError(null)}
                  aria-label={t('filamentImageErrorDismiss')}
                  title={t('filamentImageErrorDismiss')}
                  className="w-6 h-6 flex-none rounded grid place-items-center text-[var(--ink-2)] hover:text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
                >
                  <Icon name="close" size={14} />
                </button>
              </div>
            )}
          </div>

          <div>
            <p className="text-compact-meta ui-label text-[var(--ink-3)] mb-2">
              {t('filamentSectionIdentification')}
            </p>
            <div className="flex flex-col gap-2.5">
              <div>
                <label className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t('filamentMaterialLabel')}</label>
                <AutocompleteInput
                  maxSuggestions={Infinity}
                  value={form.material}
                  onChange={(v) => setForm((f) => ({ ...f, material: v }))}
                  options={resin ? RESIN_MATERIALS : FILAMENT_MATERIALS}
                  placeholder={t('filamentMaterialLabel')}
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t('filamentManufacturerLabel')}</label>
                <AutocompleteInput
                  maxSuggestions={Infinity}
                  value={form.manufacturer}
                  onChange={(v) => setForm((f) => ({ ...f, manufacturer: v }))}
                  options={manufacturerSuggestions(form.kind, knownManufacturers)}
                  placeholder={t('filamentManufacturerLabel')}
                  className={fieldClass}
                />
              </div>
              <div>
                <label className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t('filamentColorNameLabel')}</label>
                <input
                  value={form.color}
                  onChange={(e) => setForm((f) => ({ ...f, color: e.target.value }))}
                  placeholder={t('filamentColorNameLabel')}
                  className={fieldClass}
                />
              </div>
              <div role="group" aria-labelledby={`${fieldId}-colors`}>
                <span id={`${fieldId}-colors`} className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t('filamentColorsSection')}</span>
                <ColorPicker value={form.colorHex} onChange={(colorHex) => setForm((f) => ({ ...f, colorHex }))} />
                {form.extraColors.map((text, index) => {
                  const n = index + 2;
                  const bad = extraInvalid[index];
                  return (
                    <div key={index} className="mt-2">
                      <div className="flex items-center gap-2">
                        <span
                          className="w-7 h-7 rounded-[4px] border border-[var(--line-strong)] flex-none"
                          style={isValidColorHex(normalizeHex(text)) ? { backgroundColor: normalizeHex(text) } : undefined}
                          aria-hidden
                        />
                        <input
                          id={`${fieldId}-extra-${index}`}
                          value={text}
                          onChange={(e) => setForm((f) => ({ ...f, extraColors: f.extraColors.map((c, i) => (i === index ? e.target.value : c)) }))}
                          placeholder="#1a1a1a"
                          maxLength={7}
                          aria-label={t('filamentColorN').replace('{n}', String(n))}
                          aria-invalid={bad ? true : undefined}
                          aria-describedby={bad ? `${fieldId}-extra-${index}-error` : undefined}
                          className={`w-28 h-8 px-2 rounded-md border bg-[var(--panel)] text-[var(--ink)] font-medium tabular-nums text-small outline-0 focus:border-[var(--accent)] ${bad ? 'border-[var(--crit)]' : 'border-[var(--line-strong)]'}`}
                        />
                        <button
                          type="button"
                          onClick={() => removeColor(index)}
                          aria-label={t('filamentColorRemoveN').replace('{n}', String(n))}
                          title={t('filamentColorRemoveN').replace('{n}', String(n))}
                          className="w-7 h-7 rounded-md grid place-items-center text-[var(--ink-3)] hover:bg-[var(--panel-2)] hover:text-[var(--ink)] cursor-pointer"
                        >
                          <Icon name="close" size={14} />
                        </button>
                      </div>
                      {bad && (
                        <p id={`${fieldId}-extra-${index}-error`} role="alert" className="mt-1 text-caption text-[var(--crit)]">{t('filamentColorInvalid')}</p>
                      )}
                    </div>
                  );
                })}
                {form.extraColors.length + 1 < MAX_COLORS && (
                  <button
                    type="button"
                    id={`${fieldId}-add-color`}
                    onClick={addColor}
                    className="mt-2 h-7 px-2.5 rounded-md border border-dashed border-[var(--line-strong)] text-caption font-semibold text-[var(--ink-2)] hover:border-[var(--accent)] hover:text-[var(--accent)] cursor-pointer"
                  >
                    {t('filamentColorAdd')}
                  </button>
                )}
                {form.extraColors.length > 0 && (
                  <p className="text-caption text-[var(--ink-3)] mt-1 leading-snug">{t('filamentColorFirstHint')}</p>
                )}
              </div>

              {!editing && (
                <div>
                  <label className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t(resin ? 'resinQuantityLabel' : 'filamentQuantityLabel')}</label>
                  <div className="flex items-center gap-1.5">
                    <button
                      type="button"
                      onClick={() => setForm((f) => ({ ...f, quantity: String(Math.max(1, (parseInt(f.quantity, 10) || 1) - 1)) }))}
                      className="w-9 h-9 flex-none rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-title font-bold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      <Icon name="minus" size={14} />
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
                      className="w-9 h-9 flex-none rounded-md border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-title font-bold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                    >
                      <Icon name="plus" size={14} />
                    </button>
                  </div>
                  <p className="text-caption text-[var(--ink-3)] mt-1 leading-snug">{t(resin ? 'resinQuantityHint' : 'filamentQuantityHint')}</p>
                </div>
              )}
            </div>
          </div>

          <div>
            <p className="text-compact-meta ui-label text-[var(--ink-3)] mb-2">
              {t('filamentSectionStorage')}
            </p>
            <label className="block text-caption font-semibold text-[var(--ink-2)] mb-1">
              {editing?.unitId ? t('filamentHomeLocationLabel') : t('filamentLocationLabel')}
            </label>
            <AutocompleteInput
              maxSuggestions={Infinity}
              value={form.location}
              onChange={(v) => setForm((f) => ({ ...f, location: v }))}
              options={knownLocations}
              placeholder={t('filamentLocationLabel')}
              className={fieldClass}
            />
          </div>

          <div>
            <p className="text-compact-meta ui-label text-[var(--ink-3)] mb-2">
              {t('filamentSectionStock')}
            </p>
            <div className="grid grid-cols-2 gap-2.5">
              {!resin && (
                <div>
                  <label htmlFor={`${fieldId}-diameterMm`} className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t('filamentDiameterLabel')}</label>
                  <input
                    type="text"
                    inputMode="decimal"
                    value={form.diameterMm}
                    onChange={(e) => setForm((f) => ({ ...f, diameterMm: e.target.value }))}
                    {...numericProps('diameterMm')}
                  />
                  {fieldError('diameterMm')}
                </div>
              )}
              <div>
                <label htmlFor={`${fieldId}-price`} className="block text-caption font-semibold text-[var(--ink-2)] mb-1">{t('filamentPriceLabel')}</label>
                <input
                  type="text"
                  inputMode="decimal"
                  value={form.price}
                  onChange={(e) => setForm((f) => ({ ...f, price: e.target.value }))}
                  placeholder="24.90"
                  {...numericProps('price')}
                />
                {fieldError('price')}
              </div>
              <div>
                <label htmlFor={`${fieldId}-originalWeightG`} className="block text-caption font-semibold text-[var(--ink-2)] mb-1">
                  {resin ? t('resinAmountLabel') : t('filamentOriginalWeightLabel')}
                </label>
                <input
                  type="text"
                  inputMode="decimal"
                  value={form.originalWeightG}
                  onChange={(e) => {
                    setWeightTouched((w) => ({ ...w, original: true }));
                    setForm((f) => ({ ...f, originalWeightG: e.target.value }));
                  }}
                  {...numericProps('originalWeightG')}
                />
                {fieldError('originalWeightG')}
                {!editing && !weightTouched.original && weightSuggestion.source !== 'default' && (
                  <p className="mt-1 text-caption text-[var(--ink-3)]">
                    {weightSuggestion.source === 'manufacturerMaterial'
                      ? t('filamentWeightHintMakerMaterial').replace('{manufacturer}', weightSuggestion.manufacturer).replace('{material}', weightSuggestion.material)
                      : t('filamentWeightHintMaker').replace('{manufacturer}', weightSuggestion.manufacturer)}
                  </p>
                )}
              </div>
              <div>
                <label htmlFor={`${fieldId}-remainingWeightG`} className="block text-caption font-semibold text-[var(--ink-2)] mb-1">
                  {resin ? t('resinRemainingLabel') : t('filamentRemainingWeightLabel')}
                </label>
                <input
                  type="text"
                  inputMode="decimal"
                  value={form.remainingWeightG}
                  onChange={(e) => {
                    setWeightTouched((w) => ({ ...w, remaining: true }));
                    setForm((f) => ({ ...f, remainingWeightG: e.target.value }));
                  }}
                  {...numericProps('remainingWeightG')}
                />
                {fieldError('remainingWeightG')}
              </div>
            </div>
            <div className="flex flex-col gap-1.5 mt-3">
              <div className="h-1.5 rounded-full bg-[var(--plate)] overflow-hidden">
                <div
                  className="h-full rounded-full transition-[width]"
                  style={{ width: `${previewPct}%`, background: barColor }}
                />
              </div>
              <div className="tabular-nums text-body font-bold" style={{ color: barColor }}>
                {previewPct}&nbsp;%
              </div>
            </div>
          </div>
        </div>

        <div className="flex-none px-4 py-3.5 border-t border-[var(--line)] bg-[var(--panel-2)]">
          <button
            onClick={submit}
            disabled={!canEdit || busy || readingImage || !form.material.trim()}
            className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] w-full h-10 rounded-md border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-body font-bold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
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
