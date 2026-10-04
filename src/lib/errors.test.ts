import { describe, expect, it } from 'vitest';
import { expectedError, messageOf, toAppError } from './errors';

describe('messageOf', () => {
  it('reads the message field of a CmdError', () => {
    expect(messageOf({ message: 'x', expected: true })).toBe('x');
  });

  it('returns a plain string unchanged', () => {
    expect(messageOf('y')).toBe('y');
  });

  it('reads the message of an Error object', () => {
    expect(messageOf(new Error('z'))).toBe('z');
  });
});

describe('errors', () => {
  it('reads structured command errors', () => {
    expect(messageOf({ message: 'Name existiert', expected: true })).toBe('Name existiert');
    expect(toAppError({ message: 'x', expected: true })).toEqual({ message: 'x', unexpected: false });
    expect(toAppError({ message: 'y', expected: false })).toEqual({ message: 'y', unexpected: true });
  });

  it('treats plain strings and Error objects as unexpected', () => {
    expect(toAppError('boom')).toEqual({ message: 'boom', unexpected: true });
    expect(toAppError(new Error('bad'))).toEqual({ message: 'bad', unexpected: true });
  });

  it('builds expected errors for frontend validation', () => {
    expect(expectedError('Bitte Ordner wählen')).toEqual({ message: 'Bitte Ordner wählen', unexpected: false });
  });

  it('falls back to a string message property without an expected flag', () => {
    expect(messageOf({ message: 'x' })).toBe('x');
    expect(toAppError({ message: 'x' })).toEqual({ message: 'x', unexpected: true });
  });
});

it('preserves geometry codes and the expected flag without reading messages', () => {
  expect(toAppError({ message: 'anything', code: 'tooLarge', expected: true }))
    .toEqual({ message: 'anything', code: 'tooLarge', unexpected: false });
  expect(toAppError({ message: 'anything', code: 'unreadable', expected: false }))
    .toEqual({ message: 'anything', code: 'unreadable', unexpected: true });
  expect(toAppError({ message: 'file too large', expected: false }).code).toBeUndefined();
});
