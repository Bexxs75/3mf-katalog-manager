import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useT } from '../i18n/LanguageContext';

interface CloudEntryDto {
  id: string;
  name: string;
  isFolder: boolean;
  modifiedTime: string;
  sizeBytes: number | null;
}

interface Crumb {
  id: string | null;
  name: string;
}

interface Props {
  fileName: string;
  onClose: () => void;
  onConfirm: (folderId: string | null) => Promise<void>;
}

export function CloudFolderPickerDialog({ fileName, onClose, onConfirm }: Props) {
  const t = useT();
  const [crumbs, setCrumbs] = useState<Crumb[]>([{ id: null, name: 'Google Drive' }]);
  const [entries, setEntries] = useState<CloudEntryDto[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [uploading, setUploading] = useState(false);

  const currentFolderId = crumbs[crumbs.length - 1].id;

  useEffect(() => {
    setLoading(true);
    invoke<CloudEntryDto[]>('browse_cloud_folder', { folderId: currentFolderId })
      .then((result) => {
        const sorted = [...result].sort((a, b) => {
          if (a.isFolder !== b.isFolder) return a.isFolder ? -1 : 1;
          return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' });
        });
        setEntries(sorted);
        setError(null);
      })
      .catch((e) => {
        console.error('[cloud] Ordner konnte nicht geladen werden:', e);
        setError(String(e));
      })
      .finally(() => setLoading(false));
  }, [currentFolderId]);

  const openFolder = (entry: CloudEntryDto) => {
    setCrumbs((prev) => [...prev, { id: entry.id, name: entry.name }]);
  };

  const goToCrumb = (index: number) => {
    setCrumbs((prev) => prev.slice(0, index + 1));
  };

  const handleUploadHereClick = () => {
    setUploading(true);
    onConfirm(currentFolderId).finally(() => setUploading(false));
  };

  return (
    <div className="fixed inset-0 z-50 grid place-items-center bg-black/50">
      <div className="w-[480px] max-h-[560px] flex flex-col bg-[var(--panel)] border border-[var(--line)] rounded shadow-[var(--shadow)]">
        <div className="flex-none px-4 py-3 border-b border-[var(--line)] text-[13px] font-semibold break-words">
          {t('cloudFolderPickerTitle').replace('{name}', fileName)}
        </div>

        <div className="flex-none flex items-center gap-1 px-4 py-2 border-b border-[var(--line)] font-mono-ui text-[11px] text-[var(--ink-2)] overflow-x-auto whitespace-nowrap">
          {crumbs.map((crumb, index) => (
            <span key={crumb.id ?? 'root'} className="flex items-center gap-1">
              {index > 0 && <span className="text-[var(--ink-3)]">/</span>}
              <span
                onClick={() => goToCrumb(index)}
                className={`cursor-pointer ${
                  index === crumbs.length - 1 ? 'text-[var(--ink)]' : 'hover:text-[var(--accent)]'
                }`}
              >
                {crumb.name}
              </span>
            </span>
          ))}
        </div>

        <div className="flex-1 overflow-y-auto px-2 py-2">
          {error ? (
            <div className="px-2 py-4 text-[12.5px] text-[var(--accent)] break-words">
              {t('cloudBrowserError')} {error}
            </div>
          ) : loading ? (
            <div className="px-2 py-4 text-[12.5px] text-[var(--ink-3)]">{t('cloudBrowserLoading')}</div>
          ) : entries.length === 0 ? (
            <div className="px-2 py-4 text-[12.5px] text-[var(--ink-3)]">{t('cloudBrowserEmpty')}</div>
          ) : (
            entries.map((entry) => (
              <div
                key={entry.id}
                onClick={() => entry.isFolder && openFolder(entry)}
                className={`flex items-center gap-2 h-8 px-2 rounded-[3px] text-[13px] ${
                  entry.isFolder
                    ? 'cursor-pointer hover:bg-[var(--panel-2)]'
                    : 'text-[var(--ink-3)] cursor-default'
                }`}
              >
                <span className="flex-1 overflow-hidden text-ellipsis whitespace-nowrap">{entry.name}</span>
                {entry.isFolder && (
                  <span className="font-mono-ui text-[11px] text-[var(--ink-3)]">▸</span>
                )}
              </div>
            ))
          )}
        </div>

        <div className="flex-none flex gap-2 px-4 py-3 border-t border-[var(--line)] bg-[var(--panel-2)]">
          <button
            onClick={onClose}
            disabled={uploading}
            className="flex-1 h-8 rounded-[3px] border border-[var(--line-strong)] bg-[var(--panel)] text-[var(--ink)] text-[12.5px] font-semibold cursor-pointer hover:border-[var(--accent)] hover:text-[var(--accent)] disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {t('cancel')}
          </button>
          <button
            onClick={handleUploadHereClick}
            disabled={uploading}
            className="flex-1 h-8 rounded-[3px] border border-[var(--accent)] bg-[var(--accent)] text-[var(--accent-ink)] text-[12.5px] font-semibold cursor-pointer disabled:opacity-40 disabled:cursor-not-allowed"
          >
            {uploading ? t('cloudFolderPickerUploading') : t('cloudFolderPickerUploadHereButton')}
          </button>
        </div>
      </div>
    </div>
  );
}
