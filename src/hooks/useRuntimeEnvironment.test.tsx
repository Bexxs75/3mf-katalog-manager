import { StrictMode } from 'react';
import { render, renderHook, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { RuntimeEnvironmentProvider, useRuntimeEnvironment } from './useRuntimeEnvironment';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('runtime environment', () => {
  it('provides an injected container environment', () => {
    const { result } = renderHook(useRuntimeEnvironment, { wrapper: ({ children }) =>
      <RuntimeEnvironmentProvider value={{ container: true }}>{children}</RuntimeEnvironmentProvider> });
    expect(result.current.container).toBe(true);
  });

  it('waits before mounting consumers and queries only once under StrictMode', async () => {
    let resolve!: (value: { container: boolean }) => void;
    vi.mocked(invoke).mockImplementation(() => new Promise(r => { resolve = r; }));
    function Consumer() { return <span>{useRuntimeEnvironment().container ? 'container' : 'desktop'}</span>; }
    render(<StrictMode><RuntimeEnvironmentProvider><Consumer /></RuntimeEnvironmentProvider></StrictMode>);
    expect(screen.queryByText('desktop')).not.toBeInTheDocument();
    expect(invoke).toHaveBeenCalledTimes(1);
    resolve({ container: true });
    await waitFor(() => expect(screen.getByText('container')).toBeInTheDocument());
  });
});
