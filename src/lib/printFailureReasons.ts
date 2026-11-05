import gold from './printFailureReasons.json';

/** Why a print ended early, in the order the interface lists them. The same list is checked in Rust. */
export const PRINT_FAILURE_REASONS = gold.reasons;
export type PrintFailureReason = (typeof PRINT_FAILURE_REASONS)[number];
/** Longest note, in characters. */
export const MAX_FAILURE_NOTE_CHARS: number = gold.maxNoteChars;
