import { invoke } from '@tauri-apps/api/core';

export interface LinuxMenuStatus {
  /** Linux AppImage run; otherwise the feature stays hidden. */
  available: boolean;
  installed: boolean;
  /** Another tool already provides a menu entry for this AppImage. */
  thirdParty: boolean;
  /** A start-menu file of that name exists that this app did not write. */
  conflict: boolean;
  shouldPrompt: boolean;
}

export type LinuxMenuAnswer = 'yes' | 'not_now' | 'never';

/** Unknown or malformed replies count as "not available", so nothing is shown. */
export function normalizeLinuxMenuStatus(value: unknown): LinuxMenuStatus | null {
  const v = value as Partial<LinuxMenuStatus> | null;
  if (!v || typeof v !== 'object' || v.available !== true) return null;
  return {
    available: true,
    installed: v.installed === true,
    thirdParty: v.thirdParty === true,
    conflict: v.conflict === true,
    shouldPrompt: v.shouldPrompt === true,
  };
}

export const getLinuxMenuStatus = () =>
  invoke<unknown>('get_linux_menu_status').then(normalizeLinuxMenuStatus);

export const setLinuxMenuEnabled = (enabled: boolean) =>
  invoke<unknown>('set_linux_menu_enabled', { enabled }).then(normalizeLinuxMenuStatus);

export const answerLinuxMenuPrompt = (answer: LinuxMenuAnswer) =>
  invoke<unknown>('answer_linux_menu_prompt', { answer }).then(normalizeLinuxMenuStatus);
