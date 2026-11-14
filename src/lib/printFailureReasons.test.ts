import { expect, it } from 'vitest';
import gold from './printFailureReasons.json';
import { MAX_FAILURE_NOTE_CHARS, PRINT_FAILURE_REASONS } from './printFailureReasons';

// The Rust side reads the same file (katalog-core printer_link/failure.rs), so the two lists cannot drift apart.
it('lists each reason once as a lower-case code and ends with the catch-all', () => {
  expect(new Set(PRINT_FAILURE_REASONS).size).toBe(PRINT_FAILURE_REASONS.length);
  expect(PRINT_FAILURE_REASONS.every(code => /^[a-z]+(_[a-z]+)*$/.test(code))).toBe(true);
  expect(PRINT_FAILURE_REASONS[PRINT_FAILURE_REASONS.length - 1]).toBe('other');
  expect(PRINT_FAILURE_REASONS).toEqual(gold.reasons);
});

it('knows the note limit of the database layer', () => {
  expect(MAX_FAILURE_NOTE_CHARS).toBe(200);
});
