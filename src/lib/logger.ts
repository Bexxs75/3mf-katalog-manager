import { error as tauriError, info as tauriInfo } from '@tauri-apps/plugin-log';
export const error: typeof tauriError = import.meta.env.VITE_TARGET === 'web' ? async () => {} : tauriError;
export const info: typeof tauriInfo = import.meta.env.VITE_TARGET === 'web' ? async () => {} : tauriInfo;
