import { render, screen } from '@testing-library/react';
import { expect, it, vi } from 'vitest';
vi.hoisted(() => { vi.stubEnv('VITE_TARGET', 'web'); });
import { RuntimeEnvironmentProvider, useRuntimeEnvironment } from './useRuntimeEnvironment';
it('provides the read-only web environment without a request', () => {
  const fetch = vi.fn(); vi.stubGlobal('fetch', fetch);
  function Consumer() { return <div>{JSON.stringify(useRuntimeEnvironment())}</div>; }
  render(<RuntimeEnvironmentProvider><Consumer /></RuntimeEnvironmentProvider>);
  expect(screen.getByText('{"container":false,"web":true,"readOnly":true}')).toBeInTheDocument();
  expect(fetch).not.toHaveBeenCalled();
});
