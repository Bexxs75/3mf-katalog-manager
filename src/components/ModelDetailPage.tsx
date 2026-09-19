import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { ModelFile, SlicerConfig, Collection } from '../types';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { isSafeHttpUrl } from '../lib/safeUrl';
import { ModelViewer } from './ModelViewer';
import { resolveDisplayImage } from '../lib/resolveDisplayImage';
import type { DisplayPreference } from '../hooks/useDisplayPreference';
import { useEditableSourceUrl } from '../hooks/useEditableSourceUrl';
import { usePrintLog } from '../hooks/usePrintLog';
import { formatWeightG, formatLengthM, formatPrice } from '../i18n/format';

interface Props {
  model: ModelFile;
  onClose: () => void;
  onAddTag: (tag: string) => void;
  onRemoveTag: (tag: string) => void;
  onDelete: () => void;
  onTogglePrintStatus: () => void;
  onToggleFavorite: () => void;
  onToggleQueue: () => void;
  onUploadImage: () => void;
  onSnapshotCaptured: (base64: string) => void;
  onSetSourceUrl: (fileId: string, url: string | null) => void;
  onOpenInSlicer: () => void;
  onRescanMetadata: () => void;
  onAddToCollection: (collectionId: string) => void;
  collections: Collection[];
  slicers: SlicerConfig[];
  slicerError: string | null;
  rescanError: string | null;
  rescanSuccess: boolean;
  displayPreference: DisplayPreference;
}

export function ModelDetailPage({
  model,
  onClose,
  onAddTag,
  onRemoveTag,
  onDelete,
  onTogglePrintStatus,
  onToggleFavorite,
  onToggleQueue,
  onUploadImage,
  onSnapshotCaptured,
  onSetSourceUrl,
  onOpenInSlicer,
  onRescanMetadata,
  onAddToCollection,
  collections,
  slicers,
  slicerError,
  rescanError,
  rescanSuccess,
  displayPreference,
}: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [addToCollectionMenuOpen, setAddToCollectionMenuOpen] = useState(false);
  const [tagDraft, setTagDraft] = useState('');
  const {
    editing: editingSource,
    draft: sourceDraft,
    setDraft: setSourceDraft,
    startEditing: startEditingSource,
    handleKeyDown: handleSourceKeyDown,
    handleBlur: handleSourceBlur,
  } = useEditableSourceUrl(model, onSetSourceUrl);
  const resolvedImage = resolveDisplayImage(model, displayPreference);
  const [showCustomImage, setShowCustomImage] = useState(
    () => displayPreference === 'thumbnail' && resolvedImage !== null,
  );

  const rows = buildMetaRows(model, t, language);

  const submitTag = () => {
    const value = tagDraft.trim();
    if (value) onAddTag(value);
    setTagDraft('');
  };

  const {
    entries: printLogEntries,
    error: printLogError,
    addEntry: addPrintLogEntry,
    deleteEntry: deletePrintLogEntry,
  } = usePrintLog(model.id);
  const [showPrintLogForm, setShowPrintLogForm] = useState(false);
  const [printLogDate, setPrintLogDate] = useState(() => new Date().toISOString().slice(0, 10));
  const [printLogNote, setPrintLogNote] = useState('');
  const [printLogPhoto, setPrintLogPhoto] = useState<string | null>(null);

  const submitPrintLogEntry = () => {
    addPrintLogEntry(new Date(printLogDate).toISOString(), printLogNote.trim() || null, printLogPhoto)
      .then(() => {
        setShowPrintLogForm(false);
        setPrintLogNote('');
        setPrintLogPhoto(null);
        setPrintLogDate(new Date().toISOString().slice(0, 10));
      })
      .catch((e) => console.error('[print-log] Hinzufügen fehlgeschlagen:', e));
  };

  const pickPrintLogPhoto = () => {
    invoke<string | null>('pick_and_read_image').then((base64) => {
      if (base64) setPrintLogPhoto(base64);
    });
  };

  return (
    <div className="flex-1 min-w-0 overflow-y-auto p-6 flex flex-col gap-6 max-w-[1600px] w-full mx-auto">
      <header className="flex items-start gap-4">
        <button
          onClick={onClose}
          title={t('backToCatalog')}
          className="flex-none w-11 h-11 rounded-md border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] grid place-items-center hover:border-[var(--line-strong)] hover:text-[var(--ink)]"
        >
          <svg viewBox="0 0 24 24" className="w-5 h-5" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
            <path d="M19 12H5M11 18l-6-6 6-6" />
          </svg>
        </button>
        <h1 className="flex-1 min-w-0 text-[1.5rem] font-semibold leading-tight break-words">
          {model.name}
        </h1>
      </header>

      <div className="flex gap-7 flex-wrap items-start">
        <div className="flex-1 min-w-[320px]">
          <div className="relative aspect-[4/3] rounded-[10px] border border-[var(--line)] bg-[var(--plate)] overflow-hidden">
            {showCustomImage && resolvedImage ? (
              <img src={resolvedImage} alt={model.name} className="absolute inset-0 w-full h-full object-contain" />
            ) : (
              <ModelViewer
                fileId={model.id}
                needsSnapshot={model.renderSnapshotImage === null}
                onSnapshotCaptured={onSnapshotCaptured}
                showRotationControls
              />
            )}
            {resolvedImage && (
              <div className="absolute top-3 right-3 flex bg-[var(--panel-2)] border border-[var(--line)] rounded-full overflow-hidden font-mono-ui text-[11.5px]">
                <button
                  onClick={() => setShowCustomImage(false)}
                  className={`px-3.5 py-1.5 ${!showCustomImage ? 'bg-[var(--accent)] text-[var(--accent-ink)] font-semibold' : 'text-[var(--ink-3)]'}`}
                >
                  {t('detailViewer3d')}
                </button>
                <button
                  onClick={() => setShowCustomImage(true)}
                  className={`px-3.5 py-1.5 ${showCustomImage ? 'bg-[var(--accent)] text-[var(--accent-ink)] font-semibold' : 'text-[var(--ink-3)]'}`}
                >
                  {t('detailViewerImage')}
                </button>
              </div>
            )}
          </div>
          <button
            onClick={onUploadImage}
            className="mt-2 h-8 px-3 rounded-[3px] border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] text-[12px] font-semibold hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>

        <div className="w-[540px] flex-none min-w-[300px] flex flex-col gap-4">
          <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
            {rows.map((row) => (
              <div
                key={row.label}
                className="flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] last:border-b-0 text-[13.5px]"
              >
                <span className="text-[var(--ink-3)]">{row.label}</span>
                <span className="font-mono-ui tabular-nums text-right">{row.value}</span>
              </div>
            ))}
            <div className="flex justify-between items-baseline gap-4 py-2 border-b border-[var(--line)] text-[13.5px]">
              <span className="text-[var(--ink-3)]">{t('metaCreator')}</span>
              <span className="font-mono-ui text-right">{model.creator ?? t('noValue')}</span>
            </div>
            <div className="flex justify-between items-baseline gap-4 py-2 text-[13.5px]">
              <span className="text-[var(--ink-3)]">{t('metaSourceUrl')}</span>
              {editingSource ? (
                <input
                  autoFocus
                  value={sourceDraft}
                  onChange={(e) => setSourceDraft(e.target.value)}
                  onBlur={handleSourceBlur}
                  onKeyDown={handleSourceKeyDown}
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
                />
              ) : model.sourceUrl ? (
                <span className="text-right">
                  {isSafeHttpUrl(model.sourceUrl) ? (
                    <a href={model.sourceUrl} target="_blank" rel="noreferrer" className="underline decoration-[var(--line-strong)] underline-offset-2">
                      {model.sourceUrl}
                    </a>
                  ) : (
                    // Kein http(s)-Wert: als reiner Text, nie als klickbarer Link.
                    <span className="text-[var(--ink-2)]">{model.sourceUrl}</span>
                  )}{' '}
                  <button onClick={startEditingSource} className="text-[var(--ink-3)]">✎</button>
                </span>
              ) : (
                <button onClick={startEditingSource} className="text-[var(--ink-3)] underline decoration-dotted">
                  {t('sourceUrlPlaceholder')} ✎
                </button>
              )}
            </div>

            <div className="mt-4">
              <p className="font-mono-ui text-[10.5px] tracking-[0.06em] uppercase text-[var(--ink-3)] mb-2">
                {t('hashtagsHeading')}
              </p>
              <div className="flex flex-wrap gap-1.5">
                {model.tags.map((tag) => (
                  <span key={tag} className="font-mono-ui text-[12px] px-2.5 py-1 rounded-full bg-[var(--panel-2)] border border-[var(--line)] text-[var(--ink-2)]">
                    #{tag}{' '}
                    <button onClick={() => onRemoveTag(tag)} className="text-[var(--ink-3)]">✕</button>
                  </span>
                ))}
                <input
                  value={tagDraft}
                  onChange={(e) => setTagDraft(e.target.value)}
                  onKeyDown={(e) => e.key === 'Enter' && submitTag()}
                  placeholder={t('addTagPlaceholder')}
                  className="font-mono-ui text-[12px] px-2.5 py-1 rounded-full bg-transparent border border-dashed border-[var(--line)] text-[var(--ink-3)] w-32"
                />
              </div>
            </div>
          </div>

          <div className="flex flex-wrap gap-2 items-center">
            <button
              onClick={onTogglePrintStatus}
              className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full text-[12.5px] font-semibold ${
                model.printStatus === 'printed'
                  ? 'bg-[var(--good-soft,var(--accent-soft))] text-[var(--good,var(--accent))]'
                  : 'bg-[var(--panel-2)] text-[var(--ink-2)] border border-[var(--line)]'
              }`}
            >
              {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
            </button>
            <button
              onClick={onToggleFavorite}
              aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              title={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
              className={`w-9 h-9 rounded-md border grid place-items-center ${
                model.favorite ? 'border-[var(--accent)] text-[var(--accent)] bg-[var(--accent-soft)]' : 'border-[var(--line)] text-[var(--ink-2)]'
              }`}
            >
              ♥
            </button>
            <button onClick={onToggleQueue} className="text-[13px] font-semibold text-[var(--ink-2)] hover:text-[var(--ink)]">
              {model.queuePosition !== null ? t('removeFromQueue') : `+ ${t('addToQueue')}`}
            </button>
          </div>
        </div>
      </div>

      {model.sliceInfo && (
        <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
          <p className="font-mono-ui text-[10.5px] tracking-[0.06em] uppercase text-[var(--ink-3)] mb-3">
            {t('sliceFilamentHeading')}
          </p>
          <div className="flex flex-col gap-3">
            {model.sliceInfo.plates.map((plate) => (
              <div key={plate.plateIndex}>
                <p className="text-[12.5px] font-semibold text-[var(--ink-2)] mb-1.5">
                  {t('sliceFilamentPlateLabel').replace('{index}', String(plate.plateIndex))}
                </p>
                <div className="flex flex-col gap-1">
                  {plate.filaments.map((filament, i) => (
                    <div key={i} className="flex items-center gap-2 text-[13px]">
                      {filament.color && (
                        <span
                          className="w-3 h-3 rounded-full border border-[var(--line)] flex-none"
                          style={{ backgroundColor: filament.color.slice(0, 7) }}
                        />
                      )}
                      <span className="text-[var(--ink-2)]">{filament.type}</span>
                      <span className="font-mono-ui tabular-nums text-[var(--ink-3)]">
                        {formatWeightG(filament.usedG, language)} · {formatLengthM(filament.usedM, language)}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ))}
          </div>
          {model.costEstimate && (
            <div className="flex items-center justify-between gap-2 mt-3 pt-3 border-t border-[var(--line)] text-[13px]">
              <span className="text-[var(--ink-3)]">
                {t('metaCostEstimate')}
                {model.costEstimate.hasUnpricedFilaments && (
                  <span title={t('costEstimateUnpricedHint')} className="ml-1 text-[var(--ink-3)]">
                    *
                  </span>
                )}
              </span>
              <span className="font-mono-ui tabular-nums">
                {model.costEstimate.totalCost === null
                  ? t('noValue')
                  : formatPrice(model.costEstimate.totalCost, language)}
              </span>
            </div>
          )}
        </div>
      )}

      <div className="rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-5 py-4">
        <div className="flex items-center justify-between mb-3">
          <p className="font-mono-ui text-[10.5px] tracking-[0.06em] uppercase text-[var(--ink-3)]">
            {t('printLogHeading')}
          </p>
          {!showPrintLogForm && (
            <button
              onClick={() => setShowPrintLogForm(true)}
              className="text-[12px] font-semibold text-[var(--accent)] hover:underline"
            >
              {t('printLogAddButton')}
            </button>
          )}
        </div>

        {showPrintLogForm && (
          <div className="flex flex-col gap-2 mb-4 p-3 rounded-[6px] border border-dashed border-[var(--line-strong)]">
            <input
              type="date"
              value={printLogDate}
              onChange={(e) => setPrintLogDate(e.target.value)}
              className="bg-[var(--panel-2)] border border-[var(--line)] rounded px-2 py-1 text-[13px]"
            />
            <textarea
              value={printLogNote}
              onChange={(e) => setPrintLogNote(e.target.value)}
              placeholder={t('printLogNotePlaceholder')}
              rows={2}
              className="bg-[var(--panel-2)] border border-[var(--line)] rounded px-2 py-1 text-[13px] resize-none"
            />
            <div className="flex items-center gap-2">
              <button
                onClick={pickPrintLogPhoto}
                className="h-7 px-3 rounded-[3px] border border-[var(--line)] bg-transparent text-[var(--ink-2)] text-[12px] hover:border-[var(--accent)]"
              >
                {t('printLogPhotoButton')}
              </button>
              {printLogPhoto && (
                <img
                  src={`data:image/png;base64,${printLogPhoto}`}
                  alt=""
                  className="w-8 h-8 rounded object-cover border border-[var(--line)]"
                />
              )}
            </div>
            <div className="flex gap-2 justify-end">
              <button
                onClick={() => {
                  setShowPrintLogForm(false);
                  setPrintLogNote('');
                  setPrintLogPhoto(null);
                }}
                className="h-7 px-3 rounded-[3px] border border-[var(--line)] bg-transparent text-[var(--ink-2)] text-[12px]"
              >
                {t('printLogCancelButton')}
              </button>
              <button
                onClick={submitPrintLogEntry}
                className="h-7 px-3 rounded-[3px] bg-[var(--accent)] text-[var(--accent-ink)] text-[12px] font-semibold"
              >
                {t('printLogSaveButton')}
              </button>
            </div>
          </div>
        )}

        {printLogEntries.length === 0 ? (
          <p className="text-[13px] text-[var(--ink-3)]">{t('printLogEmpty')}</p>
        ) : (
          <div className="flex flex-col gap-2.5">
            {printLogEntries.map((entry) => (
              <div key={entry.id} className="flex items-start gap-2.5 text-[13px]">
                {entry.photoImage && (
                  <img
                    src={entry.photoImage}
                    alt=""
                    className="w-10 h-10 rounded object-cover border border-[var(--line)] flex-none"
                  />
                )}
                <div className="flex-1 min-w-0">
                  <div className="font-mono-ui text-[var(--ink-3)]">
                    {new Date(entry.printedAt).toLocaleDateString(language, { timeZone: 'UTC' })}
                  </div>
                  {entry.note && <div className="text-[var(--ink-2)]">{entry.note}</div>}
                </div>
                <button
                  onClick={() =>
                    deletePrintLogEntry(entry.id).catch((e) =>
                      console.error('[print-log] Löschen fehlgeschlagen:', e),
                    )
                  }
                  className="text-[var(--ink-3)] hover:text-red-400 flex-none"
                >
                  ✕
                </button>
              </div>
            ))}
          </div>
        )}
        {printLogError && <p className="text-[12.5px] text-red-400">{printLogError}</p>}
      </div>

      <footer className="flex items-center justify-between gap-4 flex-wrap rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-4 py-3.5 mt-auto">
        <span className="font-mono-ui text-[12px] text-[var(--ink-3)] break-all">{model.path}</span>
        <div className="flex gap-2.5 items-center flex-none">
          <button onClick={onDelete} className="px-4 py-2 rounded-md border border-[var(--line)] text-[13px] font-semibold text-red-400 hover:border-red-400">
            {t('delete')}
          </button>
          <button
            onClick={onRescanMetadata}
            className="px-4 py-2 rounded-md border border-[var(--line)] text-[13px] font-semibold text-[var(--ink-2)] hover:border-[var(--line-strong)]"
          >
            {t('rescanMetadataButton')}
          </button>
          <div className="relative">
            <button
              onClick={() => setAddToCollectionMenuOpen((prev) => !prev)}
              className="px-4 py-2 rounded-md border border-[var(--line)] text-[13px] font-semibold text-[var(--ink-2)] hover:border-[var(--line-strong)]"
            >
              {t('addToCollectionLabel')}
            </button>
            {addToCollectionMenuOpen && (
              <div className="absolute bottom-11 right-0 min-w-[220px] max-w-[360px] py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] z-40">
                {collections.map((c) => (
                  <button
                    key={c.id}
                    title={c.name}
                    onClick={() => {
                      onAddToCollection(c.id);
                      setAddToCollectionMenuOpen(false);
                    }}
                    className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer whitespace-nowrap overflow-hidden text-ellipsis"
                  >
                    {c.name}
                  </button>
                ))}
                {collections.length === 0 && (
                  <div className="px-3 py-1.5 font-mono-ui text-[11px] text-[var(--ink-3)]">
                    {t('noCollectionsEmptyState')}
                  </div>
                )}
              </div>
            )}
          </div>
          <button
            onClick={() => onOpenInSlicer()}
            disabled={slicers.length === 0}
            className="px-4 py-2 rounded-md bg-[var(--accent)] text-[var(--accent-ink)] text-[13px] font-semibold disabled:opacity-50"
          >
            {t('openInSlicer')} ↗
          </button>
        </div>
      </footer>
      {slicerError && <p className="text-[12.5px] text-red-400">{slicerError}</p>}
      {rescanError && <p className="text-[12.5px] text-red-400">{rescanError}</p>}
      {rescanSuccess && (
        <p className="text-[12.5px] text-[var(--good,var(--accent))]">{t('rescanMetadataSuccess')}</p>
      )}
    </div>
  );
}
