import { describe, expect, it } from 'vitest';
import { compareVersions, selectReleaseVersions, seenVersionOnClose, versionCore, releaseNoteKey } from './releaseNotesSelection';
import { releaseNotes } from './releaseNotes';
import pkg from '../../package.json';
import tauri from '../../src-tauri/tauri.conf.json';

const versions = ['0.18.1', '0.18.2', '0.18.9', '0.18.10', '0.19.0'];
describe('release selection', () => {
  it('skips fresh installs and selects only current for missing or invalid markers', () => {
    expect(selectReleaseVersions(versions, '0.18.2', null, true)).toEqual([]);
    for (const marker of [null, '', 'garbage', '0.18', '0.18.1garbage', '0.18.1\n', '0.18.1-01']) {
      expect(versionCore(marker)).toBeNull();
      expect(selectReleaseVersions(versions, '0.18.2', marker, false)).toEqual(['0.18.2']);
    }
  });
  it('includes skipped releases newest first, with numeric semver precedence', () => {
    expect(selectReleaseVersions(versions, '0.18.10', '0.18.1', false)).toEqual(['0.18.10', '0.18.9', '0.18.2']);
    expect(compareVersions('0.18.10', '0.18.9')).toBe(1);
  });
  it('skips equal and higher markers and treats prereleases as their core', () => {
    for (const marker of ['0.18.2', '0.18.3', '0.18.2-rc.1']) expect(selectReleaseVersions(versions, '0.18.2', marker, false)).toEqual([]);
    expect(selectReleaseVersions(versions, '0.18.2-rc.1', '0.18.1+build.1', false)).toEqual(['0.18.2']);
  });
  it('never moves the marker backwards, including a manual reopen after downgrade', () => {
    expect(seenVersionOnClose('0.18.2', '0.18.3')).toBe('0.18.3');
    expect(seenVersionOnClose('0.18.2', '0.18.2-rc.1')).toBe('0.18.2-rc.1');
    for (const marker of [null, 'garbage', '0.18.1']) expect(seenVersionOnClose('0.18.2', marker)).toBe('0.18.2');
  });
});
it('ships non-empty release notes in all four languages for the matching package and Tauri version', () => {
  expect(pkg.version).toBe(tauri.version);
  for (const version of [pkg.version, '0.18.2']) {
    const entry = releaseNotes[releaseNoteKey(version)];
    expect(entry).toBeDefined();
    for (const lang of ['de', 'en', 'es', 'fr'] as const) {
      const groups = entry.notes[lang];
      expect(groups).toBeDefined();
      const texts = Object.values(groups!).flat();
      expect(texts.length).toBeGreaterThan(0);
      expect(texts.every(text => text.trim().length > 0)).toBe(true);
    }
  }
});

describe('releaseNoteKey', () => {
  it('maps a pre-release to its base entry and leaves stable and unknown values alone', () => {
    expect(releaseNoteKey('0.18.6-1')).toBe('0.18.6');
    expect(releaseNoteKey('0.18.6')).toBe('0.18.6');
    expect(releaseNoteKey('garbage')).toBe('garbage');
  });
  it('every RC version of the current release finds an entry', () => {
    expect(releaseNotes[releaseNoteKey(`${releaseNoteKey(pkg.version)}-3`)]).toBeDefined();
  });
});
