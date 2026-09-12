import { useState } from 'react';
import type { ModelFile, SlicerConfig } from '../types';
import { useT, useLanguage } from '../i18n/LanguageContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { ModelViewer } from './ModelViewer';

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
  onOpenInSlicer: (slicerId?: string) => void;
  slicers: SlicerConfig[];
  slicerError: string | null;
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
  slicers,
  slicerError,
}: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [tagDraft, setTagDraft] = useState('');
  const [sourceDraft, setSourceDraft] = useState(model.sourceUrl ?? '');
  const [editingSource, setEditingSource] = useState(false);
  const [showCustomImage, setShowCustomImage] = useState(false);

  const rows = buildMetaRows(model, t, language);

  const submitTag = () => {
    const value = tagDraft.trim();
    if (value) onAddTag(value);
    setTagDraft('');
  };

  const submitSource = () => {
    const value = sourceDraft.trim();
    onSetSourceUrl(model.id, value || null);
    setEditingSource(false);
  };

  return (
    <div className="flex-1 min-w-0 overflow-y-auto p-6 flex flex-col gap-6">
      <header className="flex items-start gap-4">
        <button
          onClick={onClose}
          title={t('backToCatalog')}
          className="flex-none w-9 h-9 rounded-md border border-[var(--line)] bg-[var(--panel)] text-[var(--ink-2)] grid place-items-center hover:border-[var(--line-strong)] hover:text-[var(--ink)]"
        >
          ←
        </button>
        <h1 className="flex-1 min-w-0 text-[1.5rem] font-semibold leading-tight break-words">
          {model.name}
        </h1>
      </header>

      <div className="flex gap-7 flex-wrap items-start">
        <div className="flex-[3_1_480px] min-w-[320px]">
          <div className="relative aspect-[4/3] rounded-[10px] border border-[var(--line)] bg-[var(--plate)] overflow-hidden">
            {showCustomImage && model.displayImage ? (
              <img src={model.displayImage} alt={model.name} className="absolute inset-0 w-full h-full object-contain" />
            ) : (
              <ModelViewer
                fileId={model.id}
                needsSnapshot={model.displayImage === null}
                onSnapshotCaptured={onSnapshotCaptured}
              />
            )}
            {model.displayImage && (
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

        <div className="flex-[2_1_340px] min-w-[300px] flex flex-col gap-4">
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
                  onBlur={submitSource}
                  onKeyDown={(e) => e.key === 'Enter' && submitSource()}
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 bg-[var(--panel-2)] border border-[var(--line-strong)] rounded px-2 py-1 text-[13px]"
                />
              ) : model.sourceUrl ? (
                <span className="text-right">
                  <a href={model.sourceUrl} target="_blank" rel="noreferrer" className="underline decoration-[var(--line-strong)] underline-offset-2">
                    {model.sourceUrl}
                  </a>{' '}
                  <button onClick={() => setEditingSource(true)} className="text-[var(--ink-3)]">✎</button>
                </span>
              ) : (
                <button onClick={() => setEditingSource(true)} className="text-[var(--ink-3)] underline decoration-dotted">
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

      <footer className="flex items-center justify-between gap-4 flex-wrap rounded-[10px] border border-[var(--line)] bg-[var(--panel)] px-4 py-3.5 mt-auto">
        <span className="font-mono-ui text-[12px] text-[var(--ink-3)] break-all">{model.path}</span>
        <div className="flex gap-2.5 items-center flex-none">
          <button onClick={onDelete} className="px-4 py-2 rounded-md border border-[var(--line)] text-[13px] font-semibold text-red-400 hover:border-red-400">
            {t('delete')}
          </button>
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
    </div>
  );
}
