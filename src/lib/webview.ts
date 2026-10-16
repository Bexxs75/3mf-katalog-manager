import { getCurrentWebview as tauriWebview } from '@tauri-apps/api/webview';
// Only this capability is used by the UI. Do not construct a native Webview in web builds.
export const getCurrentWebview: () => Pick<ReturnType<typeof tauriWebview>, 'onDragDropEvent'> =
  import.meta.env.VITE_TARGET === 'web' ? () => ({ onDragDropEvent: async () => () => {} }) : tauriWebview;
