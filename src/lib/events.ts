import { listen as tauriListen } from '@tauri-apps/api/event';
export type { Event, UnlistenFn } from '@tauri-apps/api/event';
export const listen: typeof tauriListen = import.meta.env.VITE_TARGET === 'web'
  ? async () => () => {} : tauriListen;
