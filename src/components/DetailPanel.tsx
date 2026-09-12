import { useEffect, useRef, useState } from 'react';
import type { ModelFile, SlicerConfig } from '../types';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { ModelViewer } from './ModelViewer';
import { useUiDensity } from '../hooks/UiDensityContext';
import { buildMetaRows } from '../lib/modelMetadata';

interface Props {
  model: ModelFile | null;
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

export function DetailPanel({
  model,
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
  const { language } = useLanguage();
  const t = useT();
  const { density } = useUiDensity();
  const [draft, setDraft] = useState('');
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [slicerMenuOpen, setSlicerMenuOpen] = useState(false);
  const [editingSourceUrl, setEditingSourceUrl] = useState(false);
  const [sourceUrlDraft, setSourceUrlDraft] = useState('');
  const cancelingSourceUrlRef = useRef(false);
  const editingModelIdRef = useRef<string | null>(null);

  useEffect(() => {
    setConfirmDelete(false);
    setSlicerMenuOpen(false);
    setEditingSourceUrl(false);
    cancelingSourceUrlRef.current = false;
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

  const startEditingSourceUrl = () => {
    cancelingSourceUrlRef.current = false;
    editingModelIdRef.current = model.id;
    setSourceUrlDraft(model.sourceUrl ?? '');
    setEditingSourceUrl(true);
  };

  const submitSourceUrl = () => {
    const fileId = editingModelIdRef.current;
    if (!fileId) return;
    const value = sourceUrlDraft.trim();
    onSetSourceUrl(fileId, value || null);
    setEditingSourceUrl(false);
  };

  const hasSlicers = slicers.length > 0;

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
            needsSnapshot={model.displayImage === null}
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
                  onKeyDown={(e) => {
                    if (e.key === 'Enter') submitSourceUrl();
                    if (e.key === 'Escape') {
                      cancelingSourceUrlRef.current = true;
                      setEditingSourceUrl(false);
                    }
                  }}
                  onBlur={() => {
                    if (cancelingSourceUrlRef.current) {
                      cancelingSourceUrlRef.current = false;
                      return;
                    }
                    submitSourceUrl();
                  }}
                  autoFocus
                  placeholder={t('sourceUrlPlaceholder')}
                  className="flex-1 min-w-0 h-6 px-1.5 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 font-mono-ui text-xs"
                />
              ) : model.sourceUrl ? (
                <a
                  href={model.sourceUrl}
                  target="_blank"
                  rel="noreferrer"
                  className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                >
                  {model.sourceUrl}
                </a>
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
                #{tag}
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
              <div className="relative flex flex-1 min-w-0">
                <button
                  onClick={() => onOpenInSlicer()}
                  className={`flex-1 min-w-0 h-8 px-2 truncate border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] ${
                    hasSlicers ? 'rounded-l-[3px] border-r-0' : 'rounded-[3px]'
                  }`}
                >
                  {t('openInSlicer')}
                </button>
                {hasSlicers && (
                  <button
                    onClick={() => setSlicerMenuOpen((o) => !o)}
                    aria-label={t('chooseSlicerAria')}
                    className="flex items-center justify-center w-6 h-8 rounded-r-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                  >
                    <span className="text-[9px] leading-none">▾</span>
                  </button>
                )}
                {slicerMenuOpen && (
                  <div className="absolute bottom-10 left-0 w-[176px] py-1 bg-[var(--panel)] border border-[var(--line)] rounded-[3px] shadow-[var(--shadow)] z-40">
                    {slicers.map((s) => (
                      <button
                        key={s.id}
                        onClick={() => {
                          setSlicerMenuOpen(false);
                          onOpenInSlicer(s.id);
                        }}
                        className="w-full text-left px-3 py-1.5 text-[13px] text-[var(--ink)] hover:bg-[var(--panel-2)] cursor-pointer"
                      >
                        {s.name}
                      </button>
                    ))}
                  </div>
                )}
              </div>
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
            needsSnapshot={model.displayImage === null}
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
              #{tag}
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
                onKeyDown={(e) => {
                  if (e.key === 'Enter') submitSourceUrl();
                  if (e.key === 'Escape') {
                    cancelingSourceUrlRef.current = true;
                    setEditingSourceUrl(false);
                  }
                }}
                onBlur={() => {
                  if (cancelingSourceUrlRef.current) {
                    cancelingSourceUrlRef.current = false;
                    return;
                  }
                  submitSourceUrl();
                }}
                autoFocus
                placeholder={t('sourceUrlPlaceholder')}
                className="flex-1 min-w-0 px-2 py-1 rounded-md border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0"
              />
            ) : (
              <span className="flex-1 flex items-center justify-end gap-1.5 min-w-0">
                {model.sourceUrl ? (
                  <a
                    href={model.sourceUrl}
                    target="_blank"
                    rel="noreferrer"
                    className="flex-1 min-w-0 truncate text-right text-[var(--accent)] hover:underline"
                  >
                    {model.sourceUrl}
                  </a>
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
            <div className="relative flex">
              <button
                onClick={() => onOpenInSlicer()}
                className={`flex-1 h-11 px-4 flex items-center gap-2.5 justify-center font-bold cursor-pointer bg-[var(--accent)] text-[var(--accent-ink)] ${
                  hasSlicers ? 'rounded-l-lg' : 'rounded-lg'
                }`}
                style={{ fontSize: 'var(--font-size-body)' }}
              >
                🖨 {t('openInSlicer')}
              </button>
              {hasSlicers && (
                <button
                  onClick={() => setSlicerMenuOpen((o) => !o)}
                  aria-label={t('chooseSlicerAria')}
                  className="w-11 h-11 grid place-items-center rounded-r-lg bg-[var(--accent)] text-[var(--accent-ink)] cursor-pointer border-l border-[var(--accent-ink)]/20"
                >
                  ▾
                </button>
              )}
              {slicerMenuOpen && (
                <div className="absolute bottom-12 left-0 right-0 py-1.5 bg-[var(--panel)] border border-[var(--line)] rounded-lg shadow-[var(--shadow)] z-40">
                  {slicers.map((s) => (
                    <button
                      key={s.id}
                      onClick={() => {
                        setSlicerMenuOpen(false);
                        onOpenInSlicer(s.id);
                      }}
                      className="w-full text-left px-4 py-2 hover:bg-[var(--panel-2)] cursor-pointer"
                      style={{ fontSize: 'var(--font-size-body)' }}
                    >
                      {s.name}
                    </button>
                  ))}
                </div>
              )}
            </div>
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
