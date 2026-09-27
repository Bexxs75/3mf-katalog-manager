import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, waitFor } from '@testing-library/react';
import { useHasStepPreview } from './useHasStepPreview';
import * as stepPreviewApi from '../lib/api/stepPreview';

vi.mock('../lib/api/stepPreview', () => ({ hasStepPreview: vi.fn() }));

beforeEach(() => {
  vi.mocked(stepPreviewApi.hasStepPreview).mockReset();
});

describe('useHasStepPreview', () => {
  it('starts as null and resolves to what the backend reports', async () => {
    vi.mocked(stepPreviewApi.hasStepPreview).mockResolvedValue(true);
    const { result } = renderHook(() => useHasStepPreview());
    expect(result.current).toBeNull();
    await waitFor(() => expect(result.current).toBe(true));
  });

  it('resolves to false for a build without STEP support', async () => {
    vi.mocked(stepPreviewApi.hasStepPreview).mockResolvedValue(false);
    const { result } = renderHook(() => useHasStepPreview());
    await waitFor(() => expect(result.current).toBe(false));
  });

  it('fails open (true) when the IPC call itself rejects', async () => {
    vi.mocked(stepPreviewApi.hasStepPreview).mockRejectedValue(new Error('no ipc'));
    const { result } = renderHook(() => useHasStepPreview());
    await waitFor(() => expect(result.current).toBe(true));
  });
});
