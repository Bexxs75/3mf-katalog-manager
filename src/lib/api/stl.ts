import { invoke } from '../transport';

/** Which part of the model is written; the dialog offers everything or one plate. */
export type StlScope =
  | { kind: 'all' }
  | { kind: 'plate'; number: number }
  | { kind: 'parts'; indices: number[] };

export interface StlExportResult {
  /** File name of the new STL. */
  name: string;
  /** Folder it was written to. */
  folder: string;
  triangles: number;
  /** The folder lies inside the catalog folder, so "Sync with disk" will find the file. */
  inCatalogFolder: boolean;
}

// The save dialog runs in the backend and the file id replaces any path; null when the dialog was cancelled.
export function exportModelAsStl(fileId: string, scope: StlScope) {
  return invoke<StlExportResult | null>('export_model_as_stl', { fileId, scope });
}

export function showFolderInFileManager(path: string) {
  return invoke<void>('open_in_file_manager', { path });
}
