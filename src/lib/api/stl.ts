import { invoke } from '../transport';
import { readResponse } from '../webSession';

/** Which part of the model is written: everything, one plate, chosen parts, or one file per part (target = folder). */
export type StlScope =
  | { kind: 'all' }
  | { kind: 'plate'; number: number }
  | { kind: 'parts'; indices: number[] }
  | { kind: 'eachPart' };

export interface StlExportResult {
  /** File name of the new STL. */
  name: string;
  /** Folder it was written to. */
  folder: string;
  triangles: number;
  /** How many STL files were written (more than one only for `eachPart`). */
  fileCount: number;
  /** The folder lies inside the catalog folder, so "Sync with disk" will find the file. */
  inCatalogFolder: boolean;
}

// The save dialog runs in the backend and the file id replaces any path; null when the dialog was cancelled.
export function exportModelAsStl(fileId: string, scope: StlScope) {
  return invoke<StlExportResult | null>('export_model_as_stl', { fileId, scope });
}

/**
 * Web build: the server converts and sends the STL, the browser saves it. A fetch (not a plain link)
 * so an expired login shows the login page and a refusal arrives as the usual coded error.
 */
export async function downloadModelAsStl(fileId: string, modelName: string, scope: StlScope): Promise<StlExportResult> {
  const plate = scope.kind === 'plate' ? scope.number : null;
  const response = await fetch(`/api/files/${encodeURIComponent(fileId)}/stl${plate === null ? '' : `?plate=${plate}`}`, { credentials: 'same-origin' });
  const data = await readResponse<ArrayBuffer>(response);
  const stem = modelName.replace(/\.[^./\\]+$/, '') || 'model';
  const name = plate === null ? `${stem}.stl` : `${stem}_Platte${plate}.stl`;
  const url = URL.createObjectURL(new Blob([data], { type: 'model/stl' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  document.body.append(link);
  link.click();
  link.remove();
  // The click starts the save asynchronously; the address may only be released afterwards.
  setTimeout(() => URL.revokeObjectURL(url), 60_000);
  return { name, folder: '', triangles: data.byteLength > 84 ? (data.byteLength - 84) / 50 : 0, fileCount: 1, inCatalogFolder: false };
}

export function showFolderInFileManager(path: string) {
  return invoke<void>('open_in_file_manager', { path });
}
