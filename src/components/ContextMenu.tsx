import { useEffect, useRef, useState } from 'react';
import { useT } from '../i18n/LanguageContext';
import { splitFileName } from '../lib/fileName';
import { messageOf } from '../lib/errors';

interface Props {
  x: number;
  y: number;
  onClose: () => void;
  onOpenInSlicer: () => void;
  onDelete: () => void;
  onRemove?: () => Promise<void>;
  inQueue: boolean;
  onToggleQueue: () => void;
  printed: boolean;
  onTogglePrintStatus: () => void;
  currentName: string;
  onRename: (newName: string) => Promise<void>;
}

type View = 'menu' | 'confirmDelete' | 'confirmRemove' | 'rename';

export function ContextMenu({
  x,
  y,
  onClose,
  onOpenInSlicer,
  onDelete,
  onRemove,
  inQueue,
  onToggleQueue,
  printed,
  onTogglePrintStatus,
  currentName,
  onRename,
}: Props) {
  const t = useT();
  const [view, setView] = useState<View>('menu');
  const [baseNameDraft, setBaseNameDraft] = useState('');
  // Derived when the rename view opens; the extension is not editable.
  const [extension, setExtension] = useState('');
  const [renameError, setRenameError] = useState<string | null>(null);
  const [renaming, setRenaming] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    return () => { if (previous?.isConnected) previous.focus(); };
  }, []);

  useEffect(() => {
    // The first button in either confirmation is the safe cancel action.
    ref.current?.querySelector<HTMLElement>(view === 'rename' ? 'input' : 'button')?.focus();
  }, [view]);

  useEffect(() => {
    const handlePointerDown = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    };
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        if (view === 'menu') onClose();
        else { setView('menu'); setRenameError(null); }
        e.preventDefault();
        e.stopPropagation();
      }
    };
    document.addEventListener('mousedown', handlePointerDown);
    document.addEventListener('keydown', handleKeyDown);
    return () => {
      document.removeEventListener('mousedown', handlePointerDown);
      document.removeEventListener('keydown', handleKeyDown);
    };
  }, [onClose, view]);

  const submitRename = () => {
    const trimmedBase = baseNameDraft.trim();
    if (!trimmedBase) {
      setRenameError(t('renameEmptyNameError'));
      return;
    }
    const fullName = `${trimmedBase}${extension}`;
    if (fullName === currentName) {
      onClose();
      return;
    }
    setRenaming(true);
    setRenameError(null);
    onRename(fullName)
      .then(() => onClose())
      .catch((e) => {
        setRenaming(false);
        setRenameError(`${t('renameError')} ${messageOf(e)}`);
      });
  };

  return (
    <div
      ref={ref}
      className="fixed z-50 min-w-[172px] rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg overflow-hidden"
      style={{ left: x, top: y }}
    >
      {view === 'confirmRemove' ? (
        <div className="px-3 py-2.5 max-w-[260px]" aria-busy={renaming}>
          <div className="text-[12px] font-medium text-[var(--ink)] pb-1">{t('removeModelQuestion').replace('{name}', currentName)}</div>
          <div className="text-[11.5px] leading-snug text-[var(--ink-2)] pb-2">{t('removeModelHint')}</div>
          {renameError && <div role="alert" className="pb-1.5 font-mono-ui text-[10px] text-[var(--accent)] break-words">{renameError}</div>}
          <div className="flex gap-1.5">
            <button className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50" disabled={renaming} onClick={() => setView('menu')}>{t('cancel')}</button>
            <button className="flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer disabled:opacity-50" disabled={renaming} onClick={async () => {
              if (!onRemove) return;
              setRenaming(true); setRenameError(null);
              try { await onRemove(); onClose(); }
              catch (e) { setRenameError(messageOf(e)); setRenaming(false); }
            }}>{t('removeEntry')}</button>
          </div>
        </div>
      ) : view === 'confirmDelete' ? (
        <div className="px-3 py-2.5">
          <div className="text-[12px] font-medium text-[var(--ink)] pb-2">{t('deleteConfirmQuestion')}</div>
          <div className="flex gap-1.5">
            <button
              onClick={() => setView('menu')}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button
              onClick={() => {
                onDelete();
                onClose();
              }}
              className="flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer"
            >
              {t('delete')}
            </button>
          </div>
        </div>
      ) : view === 'rename' ? (
        <div className="px-3 py-2.5">
          <div className="flex items-center gap-1 mb-1.5">
            <input
              value={baseNameDraft}
              onChange={(e) => setBaseNameDraft(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') submitRename();
                if (e.key === 'Escape') {
                  e.stopPropagation();
                  setView('menu');
                  setRenameError(null);
                }
              }}
              autoFocus
              disabled={renaming}
              className="flex-1 min-w-0 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-[12.5px]"
            />
            {extension && (
              <span
                title={t('renameExtensionLockedHint')}
                className="flex-none font-mono-ui text-[12px] text-[var(--ink-3)]"
              >
                {extension}
              </span>
            )}
          </div>
          {renameError && (
            <div className="pb-1.5 font-mono-ui text-[10px] text-[var(--accent)] break-words">
              {renameError}
            </div>
          )}
          <div className="flex gap-1.5">
            <button
              onClick={() => {
                setView('menu');
                setRenameError(null);
              }}
              disabled={renaming}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[11.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button
              onClick={submitRename}
              disabled={renaming}
              className="flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[11.5px] font-semibold cursor-pointer"
            >
              {t('confirmSlicerName')}
            </button>
          </div>
        </div>
      ) : (
        <>
          <button
            onClick={() => {
              onOpenInSlicer();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('openInSlicer')}
          </button>
          <button
            onClick={() => {
              onToggleQueue();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {inQueue ? t('removeFromQueue') : t('addToQueue')}
          </button>
          <button
            onClick={() => {
              onTogglePrintStatus();
              onClose();
            }}
            className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {printed ? t('notPrintedLabel') : t('printedBadge')}
          </button>
          <button
            onClick={() => {
              const { base, extension: ext } = splitFileName(currentName);
              setBaseNameDraft(base);
              setExtension(ext);
              setView('rename');
            }}
            className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('renameLabel')}
          </button>
          {onRemove && <>
            <div className="border-t border-[var(--line)]" />
            <button onClick={() => { setRenameError(null); setView('confirmRemove'); }}
              className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]">
              {t('removeCatalog')}
            </button>
          </>}
          <button
            onClick={() => setView('confirmDelete')}
            className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('delete')}
          </button>
        </>
      )}
    </div>
  );
}
