import { useModalDialog } from '../hooks/useModalDialog';
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';
import type { ImportResultDto, Folder } from '../types';
import { toAppError, type AppError } from '../lib/errors';
import { ErrorText } from '../diagnostics/ErrorText';
import { folderNameProblem, type FolderNameProblem } from '../lib/folderName';

interface Props {
  onClose: () => void;
  onLater: () => void;
  onImported: (result: ImportResultDto) => void;
  onBaseDirSet: (path: string) => void;
}

type CatalogDirState = 'new' | 'existingDir' | 'existingFile' | 'link';

interface CatalogDirPreview {
  path: string;
  state: CatalogDirState;
}

/** A preview remembers what it was computed for, so a stale one never decides. */
interface PreviewFor extends CatalogDirPreview {
  parent: string;
  name: string;
}

const PROBLEM_TEXT: Record<
  Exclude<FolderNameProblem['kind'], 'char'>,
  'catalogSetupNameEmpty' | 'catalogSetupNameReserved' | 'catalogSetupNameTrailing' | 'catalogSetupNameTooLong'
> = {
  empty: 'catalogSetupNameEmpty',
  reserved: 'catalogSetupNameReserved',
  trailing: 'catalogSetupNameTrailing',
  tooLong: 'catalogSetupNameTooLong',
};

// States in which the name can't be used; the UI explains them in its own words.
const BLOCKING_TEXT: Partial<Record<CatalogDirState, 'catalogSetupNewExistingFile' | 'catalogSetupNewLink'>> = {
  existingFile: 'catalogSetupNewExistingFile',
  link: 'catalogSetupNewLink',
};

type Done =
  | { kind: 'adopt'; path: string; files: number; folders: number }
  | { kind: 'new'; path: string };

export function CatalogSetupDialog({ onClose, onLater, onImported, onBaseDirSet }: Props) {
  const t = useT();
  const [busy, setBusy] = useState<'adopt' | 'new' | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [done, setDone] = useState<Done | null>(null);
  // "Set up a new location" first asks for place and name; the app creates the
  // folder itself because the OS picker can't always create one (GTK on Linux).
  const [newForm, setNewForm] = useState(false);
  const [parent, setParent] = useState<string | null>(null);
  const [parentLoaded, setParentLoaded] = useState(false);
  const [name, setName] = useState(() => t('catalogSetupNewDefaultName'));
  const [preview, setPreview] = useState<PreviewFor | null>(null);
  const [previewError, setPreviewError] = useState<AppError | null>(null);
  const problem = folderNameProblem(name);
  const nameValid = problem === null;
  const previewIsCurrent = preview !== null && preview.parent === parent && preview.name === name;
  const blockingText = previewIsCurrent ? BLOCKING_TEXT[preview.state] : undefined;

  useEffect(() => {
    if (!newForm || parentLoaded) return;
    let cancelled = false;
    invoke<string | null>('default_catalog_parent')
      .then((dir) => {
        if (cancelled) return;
        setParent((current) => current ?? dir);
        setParentLoaded(true);
      })
      .catch((e) => {
        console.error('[catalog-setup] default folder unavailable:', e);
        if (!cancelled) setParentLoaded(true);
      });
    return () => {
      cancelled = true;
    };
  }, [newForm, parentLoaded]);

  const loadPreview = (forParent: string, forName: string) =>
    invoke<CatalogDirPreview>('preview_catalog_dir', { parent: forParent, name: forName }).then(
      (p): PreviewFor => ({ ...p, parent: forParent, name: forName }),
    );

  // The previous preview stays visible until the next one arrives, so the
  // result line doesn't flicker while typing.
  useEffect(() => {
    if (!newForm || !parent || !nameValid) return;
    let cancelled = false;
    loadPreview(parent, name)
      .then((p) => {
        if (cancelled) return;
        setPreview(p);
        setPreviewError(null);
      })
      .catch((e) => {
        if (cancelled) return;
        setPreview(null);
        setPreviewError(toAppError(e));
      });
    return () => {
      cancelled = true;
    };
  }, [newForm, parent, name, nameValid]);

  const adoptExisting = async () => {
    setError(null);
    try {
      const path = await invoke<string | null>('pick_folder_path');
      if (!path) return;
      setBusy('adopt');
      const before = await invoke<Folder[]>('list_folders');
      const result = await invoke<ImportResultDto>('import_dropped', { paths: [path] });
      const after = await invoke<Folder[]>('list_folders');
      const newFolders = after.filter((f) => !before.some((b) => b.id === f.id)).length;
      // Create a folder row even without models - otherwise the location
      // doesn't count as an extraction target, for example (idempotent).
      await invoke('register_catalog_base_dir', { path });

      onBaseDirSet(path);
      onImported(result);
      setDone({ kind: 'adopt', path, files: result.imported.length, folders: newFolders });
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(null);
    }
  };

  const registerNew = async (path: string) => {
    await invoke('register_catalog_base_dir', { path });
    onBaseDirSet(path);
    setDone({ kind: 'new', path });
  };

  // Picks an existing folder directly and uses it as it is.
  const setupNewFromPicker = async () => {
    setError(null);
    try {
      const path = await invoke<string | null>('pick_folder_path');
      if (!path) return;
      setBusy('new');
      await registerNew(path);
    } catch (e) {
      setError(toAppError(e));
    } finally {
      setBusy(null);
    }
  };

  const pickParent = async () => {
    setError(null);
    try {
      const path = await invoke<string | null>('pick_folder_path');
      if (path) setParent(path);
    } catch (e) {
      setError(toAppError(e));
    }
  };

  const createAndSetupNew = async () => {
    if (!parent || problem) return;
    setError(null);
    setBusy('new');
    try {
      const path = await invoke<string>('create_catalog_dir', { parent, name });
      await registerNew(path);
    } catch (e) {
      // Something may have appeared under that name meanwhile: then the
      // translated hint from a fresh preview explains it better than the
      // backend message.
      const fresh = await loadPreview(parent, name).catch(() => null);
      if (fresh && BLOCKING_TEXT[fresh.state]) setPreview(fresh);
      else setError(toAppError(e));
    } finally {
      setBusy(null);
    }
  };

  const problemText = problem
    ? problem.kind === 'char'
      ? t('catalogSetupNameInvalidChar').replace('{char}', problem.char)
      : t(PROBLEM_TEXT[problem.kind])
    : null;

  const openFolder = (path: string) => {
    invoke('open_in_file_manager', { path }).catch((e) => console.error('[catalog-setup] opening folder failed:', e));
  };

  const dialogRef = useModalDialog({ open: true, onClose: onLater, busy: busy !== null });

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/40">
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-label={t('catalogSetupTitle')} tabIndex={-1} className="w-[560px] max-h-[80vh] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)] overflow-y-auto">
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
                onClick={() => {
                  setError(null);
                  setNewForm(true);
                }}
                disabled={busy !== null}
                aria-pressed={newForm}
                className={`text-left p-4 rounded-[10px] border-2 ${newForm ? 'border-[var(--accent)]' : 'border-[var(--line)]'} hover:border-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed cursor-pointer bg-[var(--panel-2)]`}
              >
                <div className="text-[13.5px] font-semibold mb-1.5">{t('catalogSetupNewTitle')}</div>
                <div className="text-[12px] leading-relaxed text-[var(--ink-2)]">
                  {busy === 'new' ? t('catalogSetupSettingUp') : t('catalogSetupNewDescription')}
                </div>
              </button>
            </div>
          )}

          {!done && newForm && (
            <div className="mb-4">
              <label className="block text-[12px] font-semibold text-[var(--ink-2)] mb-1.5">
                {t('catalogSetupNewParentLabel')}
              </label>
              <div className="flex gap-2 items-center">
                <div
                  data-testid="catalog-setup-parent"
                  title={parent ?? undefined}
                  className={`flex-1 min-w-0 font-mono-ui text-[12px] px-2.5 py-[7px] border border-[var(--line)] rounded-[6px] bg-[var(--panel-2)] truncate ${parent ? 'text-[var(--ink)]' : 'text-[var(--ink-3)]'}`}
                >
                  {parent ?? t('catalogSetupNewParentNone')}
                </div>
                <button
                  type="button"
                  onClick={pickParent}
                  disabled={busy !== null}
                  className="h-8 px-3 rounded-[6px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold whitespace-nowrap cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {t('catalogSetupNewPickParentButton')}
                </button>
              </div>
              <label htmlFor="catalog-setup-name" className="block text-[12px] font-semibold text-[var(--ink-2)] mt-3 mb-1.5">
                {t('catalogSetupNewNameLabel')}
              </label>
              <input
                id="catalog-setup-name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={busy !== null}
                aria-invalid={problem !== null}
                className={`w-full text-[13px] px-2.5 py-[7px] rounded-[6px] border bg-[var(--panel)] text-[var(--ink)] outline-none ${problem ? 'border-[var(--accent)]' : 'border-[var(--line-strong)] focus:border-[var(--accent)]'}`}
              />
              {problemText && <div className="mt-2 text-[12px] text-[var(--accent)]">{problemText}</div>}
              {!problem && blockingText && <div className="mt-2 text-[12px] text-[var(--accent)]">{t(blockingText)}</div>}
              {!problem && !blockingText && preview && (
                <div className="mt-3 px-3 py-2 rounded-[8px] bg-[var(--accent-soft)] text-[12.5px] text-[var(--ink)] break-all">
                  {preview.state === 'existingDir' ? t('catalogSetupNewExists') : t('catalogSetupNewWillCreate')}{' '}
                  <code className="font-mono-ui text-[12px]">{preview.path}</code>
                </div>
              )}
              {!problem && previewError && (
                <div className="mt-2 font-mono-ui text-[11px] text-[var(--accent)] break-words">
                  <ErrorText error={previewError} />
                </div>
              )}
              <div className="mt-2 text-[12px] text-[var(--ink-2)]">
                {t('catalogSetupNewPickExistingBefore')}{' '}
                <button
                  type="button"
                  onClick={setupNewFromPicker}
                  disabled={busy !== null}
                  className="p-0 border-0 bg-transparent text-inherit underline cursor-pointer hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {t('catalogSetupNewPickExistingLink')}
                </button>{' '}
                {t('catalogSetupNewPickExistingAfter')}
              </div>
            </div>
          )}

          {error && (
            <div className="mb-3 font-mono-ui text-[11px] text-[var(--accent)] break-words">
              {t('catalogSetupError')} <ErrorText error={error} />
            </div>
          )}

          {!done && (
            <>
              <div className="flex items-center justify-between">
                <button
                  type="button"
                  onClick={onLater}
                  disabled={busy !== null}
                  className="h-8 px-3 rounded-[6px] border border-[var(--line-strong)] text-[12.5px] text-[var(--ink-2)] hover:border-[var(--accent)] hover:text-[var(--accent)] focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed"
                >
                  {t('catalogSetupLater')}
                </button>
                {newForm && (
                  <span className="flex gap-2">
                    <button
                      type="button"
                      onClick={() => {
                        setError(null);
                        setNewForm(false);
                      }}
                      disabled={busy !== null}
                      className="h-8 px-3 rounded-[6px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink-2)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {t('catalogSetupBack')}
                    </button>
                    <button
                      type="button"
                      onClick={createAndSetupNew}
                      disabled={busy !== null || !parent || problem !== null || blockingText !== undefined}
                      className="h-8 px-3 rounded-[6px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {t('catalogSetupNewCreateButton')}
                    </button>
                  </span>
                )}
              </div>
              <p className="mt-3 text-[11px] text-[var(--ink-3)]">{t('catalogSetupFootnote')}</p>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
