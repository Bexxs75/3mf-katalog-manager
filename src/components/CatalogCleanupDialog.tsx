import { displayName } from '../lib/displayName';
import { Icon } from './Icon';
import { useImportLock } from '../hooks/ImportLockContext';
import { useModalDialog } from '../hooks/useModalDialog';
import { useEffect, useRef, useState, type RefObject } from 'react';
import type { CatalogIssues } from '../types';
import { useFormatCount, useT } from '../i18n/LanguageContext';
import { DiskReportUnclear } from './DiskReportUnclear';
import { ErrorText } from '../diagnostics/ErrorText';
import type { AppError } from '../lib/errors';
import type { ModelFile } from '../types';

interface Props {
  issues: CatalogIssues;
  onClose: () => void;
  onDelete: (fileIds: string[]) => void;
  /** Why the last removal failed; shown inside the dialog because it stays open for what is left. */
  error?: AppError | null;
  /** What the last removal did; the dialog stays open for what is left and says so. A new object per removal. */
  notice?: { count: number } | null;
  /** What gets the focus back on close (the button that started the scan; it is disabled while the scan runs). */
  returnFocus?: RefObject<HTMLElement | null>;
}

const parentOf = (path: string) => {
  const cut = Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
  return cut > 0 ? path.slice(0, cut) : '';
};

/**
 * Models that are missing together from one folder, at least two per group; a single missing model stays a plain row.
 * With `missingFolders` from the server only those verified folders form groups; without it (older server) it is
 * "two or more missing models below the same parent".
 */
export function groupMissingByFolder(orphaned: ModelFile[], missingFolders?: CatalogIssues['missingFolders']): { folders: { path: string; models: ModelFile[] }[]; single: ModelFile[] } {
  const folders: { path: string; models: ModelFile[] }[] = [];
  if (Array.isArray(missingFolders)) {
    const byId = new Map(orphaned.map(m => [m.id, m]));
    const grouped = new Set<string>();
    for (const entry of missingFolders) {
      const models = entry.models.flatMap(id => { const m = byId.get(id); return m && !grouped.has(id) ? [m] : []; });
      if (models.length < 2) continue;
      models.forEach(m => grouped.add(m.id));
      folders.push({ path: entry.relativePath, models });
    }
    return { folders, single: orphaned.filter(m => !grouped.has(m.id)) };
  }
  const byParent = new Map<string, ModelFile[]>();
  for (const model of orphaned) {
    const parent = parentOf(model.path);
    byParent.set(parent, [...(byParent.get(parent) ?? []), model]);
  }
  const single: ModelFile[] = [];
  for (const [path, models] of byParent) {
    if (path && models.length > 1) folders.push({ path, models }); else single.push(...models);
  }
  return { folders, single };
}

function initialSelection(issues: CatalogIssues): Set<string> {
  const checked = new Set<string>();
  const folderIds = new Set(import.meta.env.VITE_TARGET === 'web' ? groupMissingByFolder(issues.orphaned, issues.missingFolders).folders.flatMap(folder => folder.models.map(m => m.id)) : []);
  // Whole folders are removed only by their own button, never as a side effect of the main one.
  issues.orphaned.filter(m => !folderIds.has(m.id)).forEach((m) => checked.add(m.id));
  issues.duplicateGroups.forEach((group) => {
    group.slice(1).forEach((m) => checked.add(m.id));
  });
  return checked;
}

export function CatalogCleanupDialog({ issues, onClose, onDelete, error, notice, returnFocus }: Props) {
  const { lockProps } = useImportLock();
  const t = useT();
  const count = useFormatCount();
  const grouped = import.meta.env.VITE_TARGET === 'web' ? groupMissingByFolder(issues.orphaned, issues.missingFolders) : { folders: [], single: issues.orphaned };
  const [checked, setChecked] = useState<Set<string>>(() => initialSelection(issues));

  const toggle = (id: string) => {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const hasIssues = issues.orphaned.length > 0 || issues.duplicateGroups.length > 0;
  // Removed models leave the lists while the dialog stays open; only what is still listed can be selected.
  const listed = new Set([...issues.orphaned, ...issues.duplicateGroups.flat()].map(m => m.id));
  const selected = Array.from(checked).filter(id => listed.has(id));
  // Roots are named only when there is more than one, like in the comparison dialog.
  const showRoot = (issues.roots?.length ?? 0) > 1;

  const dialogRef = useModalDialog({ open: true, onClose, initialFocus: '[data-initial-focus]', returnFocus });

  // The button that was clicked is gone with its group; the focus must not fall back to the page behind.
  const handledNotice = useRef(notice);
  useEffect(() => {
    if (!notice || handledNotice.current === notice) return;
    handledNotice.current = notice;
    const dialog = dialogRef.current;
    if (dialog && !dialog.contains(document.activeElement)) {
      (dialog.querySelector<HTMLElement>('[data-initial-focus]') ?? dialog).focus({ preventScroll: true });
    }
  }, [notice, dialogRef]);

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-label={t('cleanupDialogTitle')} tabIndex={-1} className="w-[480px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] flex items-center justify-between">
          <span className="text-body font-semibold">{t('cleanupDialogTitle')}</span>
          <button type="button" onClick={onClose} aria-label={t('diskSyncClose')} className="cursor-pointer text-[var(--ink-3)] hover:text-[var(--accent)] focus-visible:outline-2 focus-visible:outline-[var(--accent)]">
            <Icon name="close" size={14} />
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3">
          {notice && <p role="status" className="pb-3 text-[length:var(--font-size-title)] text-[var(--ink-2)]">{count(t('cleanupRemovedNotice'), notice.count)}</p>}
          {!hasIssues && <div className="pb-3 text-[length:var(--font-size-title)] text-[var(--ink-3)]">{t(notice ? 'cleanupNothingLeft' : 'cleanupNoIssues')}</div>}

          {error && <div role="alert" className="pb-3 text-[length:var(--font-size-title)] text-[var(--crit)] whitespace-pre-wrap break-words"><ErrorText error={error} /></div>}
          {issues.truncated && <p role="status" className="pb-3 text-small text-[var(--ink-2)]">{t('cleanupTruncated')}</p>}

          {grouped.folders.map(folder => (
            <section key={folder.path} aria-labelledby={`cleanup-folder-${folder.path}`} className="pb-4">
              <h3 id={`cleanup-folder-${folder.path}`} className="ui-label text-[var(--ink-3)] pb-1">{count(t('cleanupFolderMissing'), folder.models.length)}</h3>
              <p className="font-code text-small text-[var(--ink-2)] break-all">{displayName(folder.path)}</p>
              <p className="text-small text-[var(--ink-2)] py-1">{t('cleanupFolderHint')}</p>
              <details className="pb-2">
                <summary className="cursor-pointer text-small text-[var(--accent)]">{folder.models.map(m => displayName(m.name)).slice(0, 1)[0]} …</summary>
                <ul className="pt-1 text-small">{folder.models.map(m => <li key={m.id} className="truncate">{displayName(m.name)}</li>)}</ul>
              </details>
              <button type="button" {...lockProps} onClick={() => onDelete(folder.models.map(m => m.id))}
                className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-title)] cursor-pointer hover:border-[var(--accent)]">
                {count(t('cleanupFolderRemove'), folder.models.length)}
              </button>
            </section>
          ))}

          {grouped.single.length > 0 && (
            <div className="pb-4">
              <div className="ui-label text-[var(--ink-3)] pb-2">
                {t('cleanupOrphanedHeading')}
              </div>
              {grouped.single.map((model) => (
                <label key={model.id} className="flex items-center gap-2 py-1 text-[length:var(--font-size-title)] cursor-pointer">
                  <input type="checkbox" checked={checked.has(model.id)} onChange={() => toggle(model.id)} />
                  <span className="flex-1 truncate">{displayName(model.name)}</span>
                  <span className="font-code text-[var(--ink-3)] truncate max-w-[160px]">
                    {displayName(model.path)}
                  </span>
                </label>
              ))}
            </div>
          )}

          {issues.duplicateGroups.map((group, groupIndex) => (
            <div key={groupIndex} className="pb-4">
              <div className="ui-label text-[var(--ink-3)] pb-2">
                {t('cleanupDuplicateGroupHeading')} {groupIndex + 1}
              </div>
              {group.map((model, index) => (
                <label
                  key={model.id}
                  className={`flex items-center gap-2 py-1 text-[length:var(--font-size-title)] ${index === 0 ? 'opacity-60' : 'cursor-pointer'}`}
                >
                  <input
                    type="checkbox"
                    checked={checked.has(model.id)}
                    disabled={index === 0}
                    onChange={() => toggle(model.id)}
                  />
                  <span className="flex-1 min-w-0">
                    <span className="block truncate">{displayName(model.name)}</span>
                    <span className="block font-code text-[var(--ink-3)] break-all">{displayName(model.path)}</span>
                  </span>
                  {index === 0 && (
                    <span className="font-medium tabular-nums text-[length:var(--font-size-meta)] text-[var(--ink-3)]">{t('cleanupKeepOldest')}</span>
                  )}
                </label>
              ))}
            </div>
          ))}
          <DiskReportUnclear rows={(issues.unclear ?? []).map((u, i) => ({ key: `${i}`, path: showRoot && u.root !== undefined && issues.roots?.[u.root] ? `${issues.roots[u.root]}: ${displayName(u.path)}` : displayName(u.path), reason: u.reason }))} />
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] flex justify-end gap-2">
          <button
            data-initial-focus
            onClick={onClose}
            className="h-8 px-3 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[length:var(--font-size-title)] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
          >
            {hasIssues ? t('cancel') : t('diskSyncClose')}
          </button>
          <button
            onClick={() => onDelete(selected)}
            disabled={selected.length === 0} {...lockProps}
            className={`hover:bg-[var(--panel-2)] hover:text-[var(--ink)] h-8 px-3 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[length:var(--font-size-title)] font-semibold ${
              selected.length === 0 ? 'opacity-40 cursor-not-allowed' : 'cursor-pointer'
            }`}
          >
            {t('cleanupDeleteSelected')}
          </button>
        </div>
      </div>
    </div>
  );
}
