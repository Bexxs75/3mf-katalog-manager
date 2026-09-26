import { useEffect, useState } from 'react';
import type { ModelFile } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';
import { useUiDensity } from '../hooks/UiDensityContext';
import { buildMetaRows } from '../lib/modelMetadata';
import { isSafeHttpUrl } from '../lib/safeUrl';
import { formatDate } from '../i18n/format';
import { useEditableSourceUrl } from '../hooks/useEditableSourceUrl';
import { tagLabel } from '../lib/autoTags';

interface Props {
  model: ModelFile | null;
  trashMode?: boolean;
  onRestore?: () => void;
  onDeletePermanently?: () => void;
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
  slicerError: string | null;
}

export function DetailPanel({
  model,
  trashMode,
  onRestore,
  onDeletePermanently,
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
  slicerError,
}: Props) {
  const { language } = useLanguage();
  const t = useT();
  const { density } = useUiDensity();
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const {
    editing: editingSourceUrl,
    draft: sourceUrlDraft,
    setDraft: setSourceUrlDraft,
    startEditing: startEditingSourceUrl,
    handleKeyDown: handleSourceUrlKeyDown,
    handleBlur: handleSourceUrlBlur,
  } = useEditableSourceUrl(model, onSetSourceUrl);

  useEffect(() => {
    setConfirmDelete(false);
  }, [model?.id]);

  if (!model) {
    return (
      <aside className="flex-none w-[336px] flex items-center justify-center bg-[var(--panel)] border-l border-[var(--line)] text-[var(--ink-3)] text-[13px] px-6 text-center">
        {t('emptyStateText')}
      </aside>
    );
  }

  const submitDraft = () => {
    const value = draft.trim().replace(/^#/, '');
    if (value) onAddTag(value);
    setDraft('');
  };

  if (trashMode) {
    const rows = buildMetaRows(model, t, language);
    const expiryDate = model.deletedAt
      ? new Date(new Date(model.deletedAt).getTime() + 7 * 24 * 60 * 60 * 1000)
      : null;
    return (
      <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
        <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
          <div className="text-[14.5px] font-semibold leading-tight break-words">{model.name}</div>
          <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] pt-1.5">{model.path}</div>
        </div>

        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <ModelViewer fileId={model.id} needsSnapshot={false} onSnapshotCaptured={() => {}} />
        </div>

        <div className="flex-1 overflow-y-auto px-4 pt-3.5 pb-1">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {rows.map((row) => (
            <div key={row.label} className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]">
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">{row.label}</span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)] flex flex-col gap-2">
          {expiryDate && (
            <p className="font-mono-ui text-[10.5px] text-[var(--ink-3)]">
              {t('trashExpiryHint').replace('{date}', formatDate(expiryDate.toISOString(), language))}
            </p>
          )}
          <div className="flex gap-2">
            <button
              onClick={onRestore}
              className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('restoreLabel')}
            </button>
            <button
              onClick={onDeletePermanently}
              className="flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
            >
              {t('deletePermanentlyLabel')}
            </button>
          </div>
        </div>
      </aside>
    );
  }

  if (density === 'compact') {
    return (
    <aside className="flex-none w-[336px] flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]">
      <div className="flex-none px-4 pt-3.5 pb-3 border-b border-[var(--line)]">
        <div className="text-[14.5px] font-semibold leading-tight break-words">{model.name}</div>
        <div className="font-mono-ui text-[10.5px] text-[var(--ink-3)] pt-1.5">{model.path}</div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] border-b border-[var(--line)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 11px)',
            }}
          />
          <ModelViewer
            fileId={model.id}
            needsSnapshot={model.renderSnapshotImage === null}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <div className="absolute left-2.5 bottom-2 font-mono-ui text-[9.5px] tracking-[0.08em] uppercase text-[var(--ink-3)] pointer-events-none">
            {t('dragToRotate')}
          </div>
          <button
            onClick={onUploadImage}
            className="absolute right-2.5 top-2.5 h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.printStatus === 'printed' ? t('printedBadge') : t('notPrintedLabel')}
          </span>
          <button
            onClick={onTogglePrintStatus}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.printStatus === 'printed' ? t('markAsNotPrinted') : t('markAsPrinted')}
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
          </span>
          <button
            onClick={onToggleFavorite}
            aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.favorite ? '♥' : '♡'}
          </button>
        </div>

        <div className="flex items-center gap-2 px-4 py-2.5 border-b border-[var(--line)]">
          <span className="flex-1 text-[12.5px] font-medium">
            {model.queuePosition !== null ? t('inQueueLabel') : t('notInQueueLabel')}
          </span>
          <button
            onClick={onToggleQueue}
            className="h-7 px-2.5 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
          </button>
        </div>

        <div className="px-4 pt-3.5 pb-1">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2">
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]"
            >
              <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
                {row.label}
              </span>
              <span className="flex-1 font-mono-ui text-xs text-right">{row.value}</span>
            </div>
          ))}
          <div className="flex items-baseline gap-3 py-1.5 border-b border-[var(--line)]">
            <span className="flex-none w-[108px] text-[12.5px] text-[var(--ink-2)]">
              {t('metaSourceUrl')}
            </span>
            <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0 font-mono-ui text-xs">
              {editingSourceUrl ? (
                <input
                  value={sourceUrlDraft}
                  onChange={(e) => setSourceUrlDraft(e.target.value)}
                  onKeyDown={handleSourceUrlKeyDown}
                  onBlur={handleSourceUrlBlur}
                  autoFocus
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-xs"
                />
              ) : isSafeHttpUrl(model.sourceUrl) ? (
                <a
                  href={model.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                >
                  {model.sourceUrl}
                </a>
              ) : model.sourceUrl ? (
                // Not an http(s) value: plain text, never a clickable link.
                <span className="flex-1 min-w-0 truncate text-right text-[var(--ink-2)]">
                  {model.sourceUrl}
                </span>
              ) : (
                <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
              )}
              <span
                onClick={startEditingSourceUrl}
                className="flex-none w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
              >
                ✎
              </span>
            </span>
          </div>
        </div>

        <div className="px-4 pt-[18px] pb-5">
          <div className="font-mono-ui text-[10px] tracking-[0.12em] uppercase text-[var(--ink-3)] pb-2.5">
            {t('hashtagsHeading')}
          </div>
          <div className="flex flex-wrap gap-1.5">
            {model.tags.map((tag) => (
              <span
                key={tag}
                className="inline-flex items-center gap-1.5 h-6 pl-2.5 pr-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)] font-mono-ui text-[11.5px]"
              >
                #{tagLabel(tag, language)}
                <span
                  onClick={() => onRemoveTag(tag)}
                  className="w-4 h-4 grid place-items-center rounded-full cursor-pointer text-[10px] hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                >
                  ✕
                </span>
              </span>
            ))}
            <input
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && submitDraft()}
              placeholder={t('addTagPlaceholder')}
              className="h-6 w-[118px] px-2.5 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-[11.5px]"
            />
          </div>
        </div>
      </div>

      <div className="flex-none px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
        {slicerError && (
          <div className="pb-2 font-mono-ui text-[10px] text-[var(--accent)] break-words">
            {t('slicerLaunchError')} {slicerError}
          </div>
        )}
        <div className="flex gap-2">
          {confirmDelete ? (
            <>
              <span className="flex-1 flex items-center text-[12.5px] font-medium text-[var(--ink)]">
                {t('deleteConfirmQuestion')}
              </span>
              <button
                onClick={() => setConfirmDelete(false)}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('cancel')}
              </button>
              <button
                onClick={() => {
                  setConfirmDelete(false);
                  onDelete();
                }}
                className="flex-none h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
              >
                {t('delete')}
              </button>
            </>
          ) : (
            <>
              <button
                onClick={() => onOpenInSlicer()}
                className="flex-1 min-w-0 h-8 px-2 truncate rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                {t('openInSlicer')}
              </button>
              <button
                onClick={() => setConfirmDelete(true)}
                aria-label={t('deleteAriaLabel')}
                className="flex-none w-[34px] h-8 grid place-items-center rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] font-mono-ui cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
              >
                ✕
              </button>
            </>
          )}
        </div>
      </div>
    </aside>
    );
  }

  return (
    <aside
      className="flex-none flex flex-col min-h-0 bg-[var(--panel)] border-l border-[var(--line)]"
      style={{ width: '380px' }}
    >
      <div className="flex-1 overflow-y-auto">
        <div className="relative aspect-[4/3] bg-[var(--plate)] overflow-hidden">
          <div
            className="absolute inset-0"
            style={{
              backgroundImage:
                'repeating-linear-gradient(135deg, var(--hatch) 0 1px, transparent 1px 12px)',
            }}
          />
          <ModelViewer
            fileId={model.id}
            needsSnapshot={model.renderSnapshotImage === null}
            onSnapshotCaptured={onSnapshotCaptured}
          />
          <button
            onClick={onUploadImage}
            className="absolute right-3 top-3 h-9 px-3.5 rounded-lg bg-[var(--panel)] shadow-[var(--shadow)] text-[var(--ink-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
            style={{ fontSize: 'var(--font-size-meta)' }}
          >
            {t('uploadModelImageLabel')}
          </button>
        </div>

        <div className="px-[18px] pt-4 pb-1 flex items-start justify-between gap-3">
          <div className="font-extrabold leading-tight break-words" style={{ fontSize: 'var(--font-size-title)' }}>
            {model.name}
          </div>
          <button
            onClick={onToggleFavorite}
            aria-label={model.favorite ? t('favoriteRemove') : t('favoriteAdd')}
            className={`flex-none text-[19px] ${model.favorite ? 'text-[var(--accent)]' : 'text-[var(--ink-3)]'}`}
          >
            {model.favorite ? '♥' : '♡'}
          </button>
        </div>
        <div className="px-[18px] pb-3.5 flex flex-wrap gap-1.5">
          {model.tags.map((tag) => (
            <span
              key={tag}
              className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full border border-[var(--accent)] bg-[var(--accent-soft)] text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              #{tagLabel(tag, language)}
              <span
                onClick={() => onRemoveTag(tag)}
                className="w-4 h-4 grid place-items-center rounded-full cursor-pointer hover:bg-[var(--accent)] hover:text-[var(--accent-ink)]"
                style={{ fontSize: 'var(--font-size-label)' }}
              >
                ✕
              </span>
            </span>
          ))}
          <input
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && submitDraft()}
            placeholder={t('addTagPlaceholder')}
            className="px-2.5 py-1 rounded-full border border-dashed border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
            style={{ fontSize: 'var(--font-size-meta)' }}
          />
        </div>

        <div className="px-[18px] pb-4 border-t border-[var(--line)] pt-3.5">
          <div
            className="font-bold uppercase tracking-wide text-[var(--ink-3)] mb-2.5"
            style={{ fontSize: 'var(--font-size-label)' }}
          >
            {t('metadataHeading')}
          </div>
          {buildMetaRows(model, t, language).map((row) => (
            <div
              key={row.label}
              className="flex justify-between py-2 border-b border-[var(--line)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              <span className="text-[var(--ink-2)]">{row.label}</span>
              <span className="font-semibold">{row.value}</span>
            </div>
          ))}
          <div className="flex justify-between items-center gap-2 py-2" style={{ fontSize: 'var(--font-size-body)' }}>
            <span className="text-[var(--ink-2)]">{t('metaSourceUrl')}</span>
            {editingSourceUrl ? (
              <input
                value={sourceUrlDraft}
                onChange={(e) => setSourceUrlDraft(e.target.value)}
                onKeyDown={handleSourceUrlKeyDown}
                onBlur={handleSourceUrlBlur}
                autoFocus
                placeholder={t('sourceUrlPlaceholder')}
                className="flex-1 min-w-0 px-2 py-1 rounded-md border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
              />
            ) : (
              <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0">
                {isSafeHttpUrl(model.sourceUrl) ? (
                  <a
                    href={model.sourceUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                  >
                    {model.sourceUrl}
                  </a>
                ) : model.sourceUrl ? (
                  // Not an http(s) value: plain text, never a clickable link.
                  <span className="flex-1 min-w-0 truncate text-right text-[var(--ink-2)]">
                    {model.sourceUrl}
                  </span>
                ) : (
                  <span className="flex-1 text-right text-[var(--ink-3)]">{t('noValue')}</span>
                )}
                <span
                  onClick={startEditingSourceUrl}
                  className="flex-none w-5 h-5 grid place-items-center rounded-full cursor-pointer text-[var(--ink-3)] hover:bg-[var(--panel-2)]"
                  style={{ fontSize: 'var(--font-size-label)' }}
                >
                  ✎
                </span>
              </span>
            )}
          </div>
        </div>
      </div>

      <div className="flex-none px-[18px] py-4 border-t border-[var(--line)] bg-[var(--panel-2)] flex flex-col gap-2">
        {slicerError && (
          <div className="text-[var(--accent)] break-words" style={{ fontSize: 'var(--font-size-meta)' }}>
            {t('slicerLaunchError')} {slicerError}
          </div>
        )}
        {confirmDelete ? (
          <div className="flex items-center gap-2">
            <span className="flex-1 font-medium" style={{ fontSize: 'var(--font-size-body)' }}>
              {t('deleteConfirmQuestion')}
            </span>
            <button
              onClick={() => setConfirmDelete(false)}
              className="h-10 px-4 rounded-lg border border-[var(--line-strong)] bg-[var(--panel)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('cancel')}
            </button>
            <button
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
              className="h-10 px-4 rounded-lg bg-[var(--accent)] text-[var(--accent-ink)] font-semibold cursor-pointer"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {t('delete')}
            </button>
          </div>
        ) : (
          <>
            <button
              onClick={() => onOpenInSlicer()}
              className="flex-1 h-11 px-4 flex items-center gap-2.5 justify-center font-bold cursor-pointer bg-[var(--accent)] text-[var(--accent-ink)] rounded-lg"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🖨 {t('openInSlicer')}
            </button>
            <button
              onClick={onToggleQueue}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              🎞 {model.queuePosition !== null ? t('removeFromQueue') : t('addToQueue')}
            </button>
            <button
              onClick={onTogglePrintStatus}
              className="h-11 px-4 flex items-center justify-center gap-2.5 rounded-lg bg-[var(--panel-2)] font-semibold cursor-pointer hover:text-[var(--accent)]"
              style={{ fontSize: 'var(--font-size-body)' }}
            >
              {model.printStatus === 'printed' ? `✓ ${t('markAsNotPrinted')}` : `${t('markAsPrinted')}`}
            </button>
            <button
              onClick={() => setConfirmDelete(true)}
              aria-label={t('deleteAriaLabel')}
              className="h-9 rounded-lg text-[var(--ink-3)] hover:text-[var(--accent)] cursor-pointer"
              style={{ fontSize: 'var(--font-size-meta)' }}
            >
              {t('deleteAriaLabel')}
            </button>
          </>
        )}
      </div>
    </aside>
  );
}
