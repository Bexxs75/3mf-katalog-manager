import { useEffect, useMemo, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useLanguage, useT } from '../i18n/LanguageContext';
import { formatBytes } from '../i18n/format';
import * as importExportApi from '../lib/api/importExport';
import { BulkCheckbox } from './BulkCheckbox';
import type {
  ArchiveImportResult,
  ArchiveInfo,
  ArchiveProgress,
  ArchiveStatus,
  ConflictMode,
} from '../types';

interface Props {
  archives: ArchiveInfo[];
  defaultTargetDir: string | null;
  onCancel: () => void;
  onDone: (result: ArchiveImportResult) => void;
}

type StatusKey =
  | 'archiveStatusNoModels'
  | 'archiveStatusTooLarge'
  | 'archiveStatusEncrypted'
  | 'archiveStatusUnreadable'
  | 'archiveStatusUnsupported';

const STATUS_KEY: Record<Exclude<ArchiveStatus, 'ok'>, StatusKey> = {
  noModels: 'archiveStatusNoModels',
  tooLarge: 'archiveStatusTooLarge',
  encrypted: 'archiveStatusEncrypted',
  unreadable: 'archiveStatusUnreadable',
  unsupported: 'archiveStatusUnsupported',
};

function fileName(path: string) {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Eigenes Radio statt <input type="radio">: native Formularelemente
 *  ignorieren das Dark-Theme (siehe BulkCheckbox). */
function RadioOption({ checked, label, onSelect, disabled }: { checked: boolean; label: string; onSelect: () => void; disabled: boolean }) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      disabled={disabled}
      onClick={onSelect}
      className="flex items-center gap-2 py-0.5 text-left text-[length:var(--font-size-meta)] text-[var(--ink-2)] disabled:opacity-60"
    >
      <span
        className={`w-3.5 h-3.5 rounded-full border grid place-items-center ${
          checked ? 'border-[var(--accent)]' : 'border-[var(--line-strong)]'
        }`}
      >
        {checked && <span className="w-1.5 h-1.5 rounded-full bg-[var(--accent)]" />}
      </span>
      {label}
    </button>
  );
}

export function ArchiveImportDialog({ archives, defaultTargetDir, onCancel, onDone }: Props) {
  const t = useT();
  const { language } = useLanguage();
  const [targetDir, setTargetDir] = useState<string | null>(defaultTargetDir);
  const [conflicts, setConflicts] = useState<Record<string, boolean>>({});
  const [modes, setModes] = useState<Record<string, ConflictMode>>({});
  const [deleteArchives, setDeleteArchives] = useState(false);
  const [running, setRunning] = useState(false);
  const [progress, setProgress] = useState<Record<string, ArchiveProgress['state']>>({});
  const [error, setError] = useState<string | null>(null);

  const extractable = useMemo(() => archives.filter((a) => a.status === 'ok'), [archives]);

  useEffect(() => {
    if (!targetDir || extractable.length === 0) {
      setConflicts({});
      return;
    }
    let cancelled = false;
    importExportApi
      .archiveTargetConflicts(targetDir, extractable.map((a) => a.suggestedFolderName))
      .then((flags) => {
        if (cancelled) return;
        setConflicts(Object.fromEntries(extractable.map((a, i) => [a.path, flags[i] ?? false])));
      })
      .catch((e) => setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [targetDir, extractable]);

  useEffect(() => {
    const unlisten = listen<ArchiveProgress>('archive-progress', (event) => {
      setProgress((prev) => ({ ...prev, [event.payload.path]: event.payload.state }));
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const chooseTarget = async () => {
    const picked = await importExportApi.pickFolderPath();
    if (picked) setTargetDir(picked);
  };

  const start = async () => {
    if (!targetDir) return;
    setRunning(true);
    setError(null);
    try {
      const requests = extractable.map((a) => ({
        path: a.path,
        folderName: a.suggestedFolderName,
        onConflict: conflicts[a.path] ? (modes[a.path] ?? 'new') : ('new' as ConflictMode),
        expectedSize: a.fileSize,
        expectedModifiedUnixMs: a.modifiedUnixMs,
      }));
      const result = await importExportApi.extractArchives(targetDir, requests, deleteArchives);
      onDone(result);
    } catch (e) {
      setError(String(e));
      setRunning(false);
    }
  };

  const progressLabel = (state: ArchiveProgress['state'] | undefined) => {
    switch (state) {
      case 'extracting':
        return t('archiveProgressExtracting');
      case 'importing':
        return t('archiveProgressImporting');
      case 'done':
        return t('archiveProgressDone');
      case 'failed':
        return t('archiveProgressFailed');
      default:
        return null;
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[520px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[14px] font-semibold">
          {t('archiveDialogTitle')}
        </div>

        <div className="flex-none px-4 py-3 border-b border-[var(--line)] flex items-center gap-2 text-[length:var(--font-size-title)]">
          <span className="text-[var(--ink-3)]">{t('archiveTargetLabel')}:</span>
          <span className="flex-1 truncate font-mono-ui text-[length:var(--font-size-meta)]">
            {targetDir ?? t('archiveTargetMissing')}
          </span>
          <button
            type="button"
            disabled={running}
            onClick={chooseTarget}
            className="px-2 py-1 rounded-[4px] border border-[var(--line)] hover:border-[var(--accent)] disabled:opacity-60"
          >
            {t('archiveTargetChange')}
          </button>
        </div>

        <div className="flex-1 overflow-y-auto px-4 py-3 flex flex-col gap-3">
          {archives.map((archive) => {
            const ok = archive.status === 'ok';
            const conflict = ok && conflicts[archive.path];
            const mode = modes[archive.path] ?? 'new';
            const state = progressLabel(progress[archive.path]);
            return (
              <div key={archive.path} className={ok ? '' : 'opacity-60'}>
                <div className="flex items-center gap-2 text-[length:var(--font-size-title)]">
                  <span className="flex-1 truncate">{fileName(archive.path)}</span>
                  <span className="text-[length:var(--font-size-meta)] text-[var(--ink-3)]">
                    {ok
                      ? t('archiveModelsCount')
                          .replace('{count}', String(archive.modelCount))
                          .replace('{size}', formatBytes(archive.unpackedSize, language))
                      : t(STATUS_KEY[archive.status as Exclude<ArchiveStatus, 'ok'>])}
                  </span>
                  {state && <span className="text-[length:var(--font-size-meta)] text-[var(--accent)]">{state}</span>}
                </div>
                {conflict && (
                  <div role="radiogroup" className="pl-3 pt-1 flex flex-col">
                    <span className="text-[length:var(--font-size-meta)] text-[var(--ink-3)]">
                      {t('archiveConflictText').replace('{name}', archive.suggestedFolderName)}
                    </span>
                    <RadioOption
                      checked={mode === 'new'}
                      disabled={running}
                      label={t('archiveConflictNew')}
                      onSelect={() => setModes((m) => ({ ...m, [archive.path]: 'new' }))}
                    />
                    <RadioOption
                      checked={mode === 'merge'}
                      disabled={running}
                      label={t('archiveConflictMerge')}
                      onSelect={() => setModes((m) => ({ ...m, [archive.path]: 'merge' }))}
                    />
                  </div>
                )}
              </div>
            );
          })}
          {error && <div className="text-[length:var(--font-size-meta)] text-[var(--danger,#d33)]">{error}</div>}
        </div>

        <div className="flex-none px-4 py-3 border-t border-[var(--line)] flex items-center gap-2">
          <BulkCheckbox checked={deleteArchives} onToggle={() => !running && setDeleteArchives((v) => !v)} />
          <span
            className="flex-1 cursor-pointer text-[length:var(--font-size-title)]"
            onClick={() => !running && setDeleteArchives((v) => !v)}
          >
            {t('archiveDeleteAfter')}
          </span>
          <button
            type="button"
            disabled={running}
            onClick={onCancel}
            className="px-3 py-1.5 rounded-[4px] border border-[var(--line)] hover:border-[var(--accent)] disabled:opacity-60"
          >
            {t('archiveCancel')}
          </button>
          <button
            type="button"
            disabled={running || !targetDir || extractable.length === 0}
            onClick={start}
            className="px-3 py-1.5 rounded-[4px] bg-[var(--accent)] text-[var(--accent-ink)] disabled:opacity-60"
          >
            {t('archiveExtractButton').replace('{count}', String(extractable.length))}
          </button>
        </div>
      </div>
    </div>
  );
}
