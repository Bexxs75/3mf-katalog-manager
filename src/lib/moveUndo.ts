import type { MoveFileResult } from './api/folders';
import type { FolderMoveBlock } from './folderTree';

/** Older steps are dropped: the list lives in memory only and should stay small. */
export const MAX_UNDO_STEPS = 20;

export type UndoStep =
  | { kind: 'moveFiles'; items: { fileId: string; fromFolderId: string }[] }
  | { kind: 'moveFolder'; folderId: string; name: string; fromParentId: string }
  | { kind: 'renameFolder'; folderId: string; oldName: string; newName: string };

export function pushUndoStep(stack: readonly UndoStep[], step: UndoStep): UndoStep[] {
  return [...stack, step].slice(-MAX_UNDO_STEPS);
}

export interface MoveSummary {
  moved: number;
  skippedNameExists: number;
  skippedAlreadyThere: number;
  failed: number;
  /** Reason code of the first failed file, shown so a single failure is not a bare number. */
  firstFailReason?: string;
}

export function summarizeMoveResults(results: readonly MoveFileResult[]): MoveSummary {
  const summary: MoveSummary = { moved: 0, skippedNameExists: 0, skippedAlreadyThere: 0, failed: 0 };
  for (const result of results) {
    if (result.status === 'moved') summary.moved++;
    else if (result.status === 'skipped') {
      if (result.reason === 'already_there') summary.skippedAlreadyThere++;
      else summary.skippedNameExists++;
    } else {
      summary.failed++;
      summary.firstFailReason ??= result.reason;
    }
  }
  return summary;
}

/**
 * Only files whose old folder is known can be put back. A file that had no
 * folder is left out of the undo step on purpose: guessing a place for it
 * would move it somewhere the user never had it.
 */
export function undoItemsFromResults(results: readonly MoveFileResult[]): { fileId: string; fromFolderId: string }[] {
  return results.flatMap((result) =>
    result.status === 'moved' && result.fromFolderId ? [{ fileId: result.id, fromFolderId: result.fromFolderId }] : []);
}

export function groupByOldFolder(items: readonly { fileId: string; fromFolderId: string }[]): Map<string, string[]> {
  const groups = new Map<string, string[]>();
  for (const item of items) groups.set(item.fromFolderId, [...(groups.get(item.fromFolderId) ?? []), item.fileId]);
  return groups;
}

export interface MoveBatchInfo {
  /** Names of (at most) the first two files that were moved, or requested when none moved. */
  names: string[];
  total: number;
  summary: MoveSummary;
}

export interface UndoOutcome {
  kind: 'files' | 'folderMove' | 'rename';
  ok: boolean;
  /** Files put back (kind "files"). */
  count?: number;
  total?: number;
  /** Folder name for folder steps; for a rename the name before the undo. */
  name?: string;
  /** For a rename: the name restored by the undo. */
  restoredName?: string;
  /** A backend reason code or a plain backend message. */
  reason?: string;
}

export interface MoveToastState {
  from: string;
  to: string;
  error?: boolean;
  unexpected?: boolean;
  collection?: 'added' | 'already';
  blocked?: FolderMoveBlock;
  batch?: MoveBatchInfo;
  undo?: UndoOutcome;
  /** An undo step was recorded for this action, so the toast offers it. */
  canUndo?: boolean;
}
