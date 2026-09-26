import { describe, expect, it } from 'vitest';
import { messageOf } from './errors';

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
