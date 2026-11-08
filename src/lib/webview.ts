import { getCurrentWebview as tauriWebview } from '@tauri-apps/api/webview';
import { NativeDragState } from './nativeDrag';
// Highlight observers may need own-drag events to clear a previous target; importers must not receive them.
// Only this capability is used by the UI. Do not construct a native Webview in web builds.
export const getCurrentWebview: (includeOwnDrag?: boolean) => Pick<ReturnType<typeof tauriWebview>, 'onDragDropEvent'> =
  import.meta.env.VITE_TARGET === 'web' ? () => ({ onDragDropEvent: async () => () => {} }) : (includeOwnDrag = false) => ({
    onDragDropEvent: handler => tauriWebview().onDragDropEvent(event => {
      if (includeOwnDrag || !NativeDragState.isActive()) handler(event);
    }),
  });
