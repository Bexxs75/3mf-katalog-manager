import { invoke } from '@tauri-apps/api/core';

export interface RuntimeEnvironment { container: boolean }

let environment: Promise<RuntimeEnvironment> | undefined;

// Share the startup request across StrictMode mounts and all consumers.
export function getRuntimeEnvironment(): Promise<RuntimeEnvironment> {
  return environment ??= invoke<RuntimeEnvironment>('get_runtime_environment')
    .catch(() => ({ container: false }));
}

export const hasNetworkFilesystemWarning = () => invoke<boolean>('has_network_filesystem_warning');
