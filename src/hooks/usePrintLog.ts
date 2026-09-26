import { useCallback, useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

export interface PrintLogEntry {
  id: string;
  printedAt: string;
  note: string | null;
  photoImage: string | null;
}

/**
 * Loads and manages the print log entries of a single model.
 * Deliberately not part of ModelFile/list_files - entries can contain
 * photos and shouldn't come along with every catalog load.
 */
export function usePrintLog(fileId: string) {
  const [entries, setEntries] = useState<PrintLogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(() => {
    setLoading(true);
    invoke<PrintLogEntry[]>('list_print_log_entries', { fileId })
      .then((result) => {
        setEntries(result);
        setError(null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, [fileId]);

  useEffect(() => {
    reload();
  }, [reload]);

  const addEntry = useCallback(
    (printedAt: string, note: string | null, photoBase64: string | null) => {
      return invoke<PrintLogEntry>('add_print_log_entry', {
        fileId,
        printedAt,
        note,
        photoBase64,
      }).then((entry) => {
        setEntries((prev) => [entry, ...prev]);
        return entry;
      });
    },
    [fileId],
  );

  const deleteEntry = useCallback((entryId: string) => {
    return invoke('delete_print_log_entry', { entryId }).then(() => {
      setEntries((prev) => prev.filter((e) => e.id !== entryId));
    });
  }, []);

  return { entries, loading, error, addEntry, deleteEntry };
}
