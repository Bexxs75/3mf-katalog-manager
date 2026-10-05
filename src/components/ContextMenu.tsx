import { RevealFileButton } from './RevealFileButton';
import { useImportLock } from '../hooks/ImportLockContext';
import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
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
  fileId: string;
  currentName: string;
  onRename: (newName: string) => Promise<void>;
}

type View = 'menu' | 'confirmDelete' | 'confirmRemove' | 'rename';

function ModelContextMenu({
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
  fileId,
  currentName,
  onRename,
}: Props) {
  const { lockProps } = useImportLock();
  const t = useT();
  const [view, setView] = useState<View>('menu');
  const [baseNameDraft, setBaseNameDraft] = useState('');
  // Derived when the rename view opens; the extension is not editable.
  const [extension, setExtension] = useState('');
  const [renameError, setRenameError] = useState<string | null>(null);
  const [renaming, setRenaming] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    ref.current?.querySelector<HTMLElement>(view === 'rename' ? 'input' : 'button')?.focus();
  }, [view]);

  const submitRename = () => {
    if (lockProps.disabled) return;
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
    <ContextMenuFrame x={x} y={y} onClose={onClose} onEscape={() => {
      if (view === 'menu') onClose();
      else { setView('menu'); setRenameError(null); }
    }}>
    <div ref={ref}>
      {view === 'confirmRemove' ? (
        <div className="px-3 py-2.5 max-w-[260px]" aria-busy={renaming}>
          <div className="text-small font-medium text-[var(--ink)] pb-1">{t('removeModelQuestion').replace('{name}', currentName)}</div>
          <div className="text-caption leading-snug text-[var(--ink-2)] pb-2">{t('removeModelHint')}</div>
          {renameError && <div role="alert" className="pb-1.5 font-medium tabular-nums text-compact-meta text-[var(--accent)] break-words">{renameError}</div>}
          <div className="flex gap-1.5">
            <button className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50" disabled={renaming} onClick={() => setView('menu')}>{t('cancel')}</button>
            <button className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer disabled:opacity-50" disabled={renaming} {...lockProps} onClick={async () => {
              if (!onRemove) return;
              setRenaming(true); setRenameError(null);
              try { await onRemove(); onClose(); }
              catch (e) { setRenameError(messageOf(e)); setRenaming(false); }
            }}>{t('removeEntry')}</button>
          </div>
        </div>
      ) : view === 'confirmDelete' ? (
        <div className="px-3 py-2.5">
          <div className="text-small font-medium text-[var(--ink)] pb-2">{t('deleteConfirmQuestion')}</div>
          <div className="flex gap-1.5">
            <button
              onClick={() => setView('menu')}
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button {...lockProps}
              onClick={() => {
                onDelete();
                onClose();
              }}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer"
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
              className="flex-1 min-w-0 h-7 px-2 rounded-[3px] border border-[var(--line-strong)] bg-transparent text-[var(--ink)] outline-0 text-small"
            />
            {extension && (
              <span
                title={t('renameExtensionLockedHint')}
                className="flex-none font-medium tabular-nums text-small text-[var(--ink-3)]"
              >
                {extension}
              </span>
            )}
          </div>
          {renameError && (
            <div className="pb-1.5 font-medium tabular-nums text-compact-meta text-[var(--accent)] break-words">
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
              className="flex-1 h-7 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-caption font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
            >
              {t('cancel')}
            </button>
            <button
              onClick={submitRename}
              disabled={renaming} {...lockProps}
              className="hover:bg-[var(--panel-2)] hover:text-[var(--ink)] flex-1 h-7 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-caption font-semibold cursor-pointer"
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
          <RevealFileButton fileId={fileId} onSuccess={onClose}
            className="w-full text-left px-3 py-2 text-title text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50" />
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
          <button {...lockProps}
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
            <button {...lockProps} onClick={() => { setRenameError(null); setView('confirmRemove'); }}
              className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]">
              {t('removeCatalog')}
            </button>
          </>}
          <button {...lockProps}
            onClick={() => setView('confirmDelete')}
            className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)]"
          >
            {t('delete')}
          </button>
        </>
      )}
    </div>
    </ContextMenuFrame>
  );
}

interface CollectionMenuProps {
  x: number;
  y: number;
  onClose: () => void;
  collectionActions: { onRename: () => void; onDelete: () => void };
}

export function ContextMenu(props: Props | CollectionMenuProps) {
  const { lockProps } = useImportLock();
  const t = useT();
  if (!('collectionActions' in props)) return <ModelContextMenu {...props} />;
  return <ContextMenuFrame x={props.x} y={props.y} onClose={props.onClose}>
    <button {...lockProps} onClick={props.collectionActions.onRename}
      className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--ink)] cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50">{t('renameLabel')}</button>
    <button {...lockProps} onClick={props.collectionActions.onDelete}
      className="w-full text-left px-3 py-2 text-[length:var(--font-size-title)] text-[var(--crit)] cursor-pointer hover:bg-[var(--panel-2)] disabled:opacity-50">{t('delete')}</button>
  </ContextMenuFrame>;
}

function ContextMenuFrame({ x, y, onClose, onEscape = onClose, children }: {
  x: number; y: number; onClose: () => void; onEscape?: () => void; children: ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    const bounds = menu.getBoundingClientRect();
    setPosition({ left: Math.max(0, Math.min(x, window.innerWidth - bounds.width)),
      top: Math.max(0, Math.min(y, window.innerHeight - bounds.height)) });
  }, [x, y, children]);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>('button:not(:disabled)')?.focus();
    return () => { if (previous?.isConnected) previous.focus(); };
  }, []);
  useEffect(() => {
    const pointer = (event: MouseEvent) => {
      if (ref.current && !ref.current.contains(event.target as Node)) onClose();
    };
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); onEscape(); }
    };
    document.addEventListener('mousedown', pointer);
    document.addEventListener('keydown', key);
    return () => { document.removeEventListener('mousedown', pointer); document.removeEventListener('keydown', key); };
  }, [onClose, onEscape]);
  return <div data-navigation-menu ref={ref}
    className="fixed z-50 min-w-[172px] max-w-[100vw] max-h-[100vh] rounded-[4px] border border-[var(--line-strong)] bg-[var(--panel)] shadow-lg overflow-auto"
    style={position}>{children}</div>;
}
