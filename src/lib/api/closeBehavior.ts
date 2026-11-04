import { invoke } from '../transport';

export type CloseAction = 'exit' | 'tray';
export type CloseBehaviorPlatform = 'windows' | 'macos' | 'linux' | 'other';

export interface CloseBehavior {
  /** The stored wish. */
  action: CloseAction;
  /** An icon exists now; only then does "tray" hide the window. */
  trayAvailable: boolean;
  /** False in the container image, where there is no notification area. */
  supported: boolean;
  /** Reported by the backend: the texts differ per system. */
  platform: CloseBehaviorPlatform;
}

const PLATFORMS: readonly CloseBehaviorPlatform[] = ['windows', 'macos', 'linux', 'other'];

/** Unknown or malformed replies count as "not supported", so the group stays hidden. */
export function normalizeCloseBehavior(value: unknown): CloseBehavior | null {
  const v = value as Partial<CloseBehavior> | null;
  if (!v || typeof v !== 'object' || v.supported !== true) return null;
  return {
    action: v.action === 'tray' ? 'tray' : 'exit',
    trayAvailable: v.trayAvailable === true,
    supported: true,
    platform: PLATFORMS.find(p => p === v.platform) ?? 'other',
  };
}

export const getCloseBehavior = () =>
  invoke<unknown>('get_close_behavior').then(normalizeCloseBehavior);

export const setCloseBehavior = (action: CloseAction) =>
  invoke<unknown>('set_close_behavior', { action }).then(normalizeCloseBehavior);

export const setTrayLabels = (open: string, quit: string) =>
  invoke<void>('set_tray_labels', { open, quit });
