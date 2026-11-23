import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import { uiSettings } from '../lib/uiSettings';
import { currentVersion, releaseNotes } from '../lib/releaseNotes';
import { compareVersions, LAST_SEEN_VERSION_KEY as key } from '../lib/releaseNotesSelection';
import { useWhatsNew } from './useWhatsNew';

beforeEach(() => { uiSettings.removeItem(key); });
afterEach(() => { uiSettings.removeItem(key); });

const previous = () => Object.keys(releaseNotes).filter(v => compareVersions(v, currentVersion) < 0).sort((a, b) => compareVersions(b, a))[0]!;
// The web build never runs the setup dialog, so firstRun stays true there; it must not decide anything.
const web = (wait = false) => renderHook(() => useWhatsNew({ wait, firstRun: true, web: true }));

describe('what is new in the web build', () => {
  it('shows nothing on the first start, writes the marker, and stays quiet on the next start', () => {
    const first = web();
    expect(first.result.current.versions).toEqual([]);
    expect(first.result.current.settled).toBe(true);
    expect(uiSettings.getItem(key)).toBe(currentVersion);
    first.unmount();
    const restart = web();
    expect(restart.result.current.versions).toEqual([]);
    expect(restart.result.current.settled).toBe(true);
  });

  it('shows the news after a version change and moves the marker only when it is closed', () => {
    uiSettings.setItem(key, previous());
    const { result } = web();
    expect(result.current.versions.length).toBeGreaterThan(0);
    expect(result.current.settled).toBe(false);
    expect(uiSettings.getItem(key)).toBe(previous());
    act(() => result.current.close());
    expect(result.current.versions).toEqual([]);
    expect(result.current.settled).toBe(true);
    expect(uiSettings.getItem(key)).toBe(currentVersion);
  });

  it('decides nothing while a start-up step is still running', () => {
    uiSettings.setItem(key, previous());
    const { result, rerender } = renderHook(({ wait }) => useWhatsNew({ wait, firstRun: true, web: true }), { initialProps: { wait: true } });
    expect(result.current.versions).toEqual([]);
    expect(result.current.settled).toBe(false);
    rerender({ wait: false });
    expect(result.current.versions.length).toBeGreaterThan(0);
  });

  it('can be reopened by hand after a fresh install without moving the marker back', () => {
    const { result } = web();
    act(() => result.current.reopen());
    expect(result.current.versions.length).toBe(1);
    act(() => result.current.close());
    expect(uiSettings.getItem(key)).toBe(currentVersion);
  });

  it('keeps a newer marker when an older build is opened and closed again', () => {
    uiSettings.setItem(key, '99.0.0');
    const { result } = web();
    expect(result.current.versions).toEqual([]);
    act(() => result.current.reopen());
    act(() => result.current.close());
    expect(uiSettings.getItem(key)).toBe('99.0.0');
  });
});
