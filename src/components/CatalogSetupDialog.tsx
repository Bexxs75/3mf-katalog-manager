import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import type { ImportResultDto, Folder } from '../types';

interface Props {
  onClose: () => void;
  onLater: () => void;
  onImported: (result: ImportResultDto) => void;
  onBaseDirSet: (path: string) => void;
}

type Done =
  | { kind: 'adopt'; path: string; files: number; folders: number }
  | { kind: 'new'; path: string };

export function CatalogSetupDialog({ onClose, onLater, onImported, onBaseDirSet }: Props) {
  const t = useT();
  const [busy, setBusy] = useState<'adopt' | 'new' | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState<Done | null>(null);

  const adoptExisting = async () => {
    setError(null);
    const path = await invoke<string | null>('pick_folder_path');
    if (!path) return;
    setBusy('adopt');
    try {
      const before = await invoke<Folder[]>('list_folders');
      const result = await invoke<ImportResultDto>('import_dropped', { paths: [path] });
      const after = await invoke<Folder[]>('list_folders');
      const newFolders = after.filter((f) => !before.some((b) => b.id === f.id)).length;
      // Auch ohne Modelle eine Ordnerzeile anlegen - sonst gilt der
      // Speicherort z.B. nicht als Entpack-Ziel (idempotent).
      await invoke('register_catalog_base_dir', { path });

      onBaseDirSet(path);
      onImported(result);
      setDone({ kind: 'adopt', path, files: result.imported.length, folders: newFolders });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const setupNew = async () => {
    setError(null);
    const path = await invoke<string | null>('pick_folder_path');
    if (!path) return;
    setBusy('new');
    try {
      await invoke('register_catalog_base_dir', { path });
      onBaseDirSet(path);
      setDone({ kind: 'new', path });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(null);
    }
  };

  const openFolder = (path: string) => {
    invoke('open_in_file_manager', { path }).catch((e) => console.error('[catalog-setup] Ordner oeffnen fehlgeschlagen:', e));
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div className="w-[560px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] overflow-y-auto">
        <div className="px-5 py-4">
          <div className="text-[16px] font-semibold mb-2.5">{t('catalogSetupTitle')}</div>
          <p className="text-[13px] leading-relaxed text-[var(--ink-2)] mb-4">{t('catalogSetupIntro')}</p>

          {!done && (
            <div className="mb-4 p-3 rounded-[8px] border border-dashed border-[var(--line-strong)] text-[12px] leading-relaxed text-[var(--ink-2)]">
              {t('catalogSetupFileTypesNote')}
            </div>
          )}

          {done ? (
            <div className="mb-4 p-4 rounded-[10px] border-2 border-[var(--good)] bg-[var(--good-soft)]">
              <div className="text-[13px] font-semibold text-[var(--good)] mb-3">
                ✓{' '}
                {done.kind === 'adopt'
                  ? t('catalogSetupAdoptSummary').replace('{files}', String(done.files)).replace('{folders}', String(done.folders))
                  : t('catalogSetupNewSummary').replace('{path}', done.path)}
              </div>
              <div className="flex gap-2">
                <button
                  onClick={() => openFolder(done.path)}
                  className="h-8 px-3 rounded-[6px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)]"
                >
                  {t('catalogSetupOpenFolderButton')}
                </button>
                <button
                  onClick={onClose}
                  className="h-8 px-3 rounded-[6px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer"
                >
                  {t('catalogSetupDoneButton')}
                </button>
              </div>
            </div>
          ) : (
            <div className="grid grid-cols-2 gap-3 mb-4">
              <button
                onClick={adoptExisting}
                disabled={busy !== null}
                className="text-left p-4 rounded-[10px] border-2 border-[var(--line)] hover:border-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer bg-[var(--panel-2)]"
              >
                <div className="text-[13.5px] font-semibold mb-1.5">{t('catalogSetupAdoptTitle')}</div>
                <div className="text-[12px] leading-relaxed text-[var(--ink-2)]">
                  {busy === 'adopt' ? t('catalogSetupImporting') : t('catalogSetupAdoptDescription')}
                </div>
              </button>
              <button
                onClick={setupNew}
                disabled={busy !== null}
                className="text-left p-4 rounded-[10px] border-2 border-[var(--line)] hover:border-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer bg-[var(--panel-2)]"
              >
                <div className="text-[13.5px] font-semibold mb-1.5">{t('catalogSetupNewTitle')}</div>
                <div className="text-[12px] leading-relaxed text-[var(--ink-2)]">
                  {busy === 'new' ? t('catalogSetupSettingUp') : t('catalogSetupNewDescription')}
                </div>
              </button>
            </div>
          )}

          {error && (
            <div className="mb-3 font-mono-ui text-[11px] text-[var(--accent)] break-words">
              {t('catalogSetupError')} {error}
            </div>
          )}

          {!done && (
            <>
              <div className="flex items-center justify-between">
                <span
                  onClick={busy === null ? onLater : undefined}
                  className={`text-[12px] text-[var(--ink-3)] underline ${busy === null ? 'cursor-pointer hover:text-[var(--accent)]' : 'opacity-50'}`}
                >
                  {t('catalogSetupLater')}
                </span>
              </div>
              <p className="mt-3 text-[11px] text-[var(--ink-3)]">{t('catalogSetupFootnote')}</p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
