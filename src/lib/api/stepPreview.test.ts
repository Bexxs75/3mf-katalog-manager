import { describe, expect, it, vi, beforeEach } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import * as stepPreviewApi from './stepPreview';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
beforeEach(() => { vi.mocked(invoke).mockReset(); });

describe('step preview api', () => {
  it('hasStepPreview', async () => {
    vi.mocked(invoke).mockResolvedValue(true);
    const actual = await stepPreviewApi.hasStepPreview();
    expect(invoke).toHaveBeenCalledWith('has_step_preview');
    expect(actual).toBe(true);
  });

  it('openStepDownload passes the current language and nothing else', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    await stepPreviewApi.openStepDownload('de');
    expect(invoke).toHaveBeenCalledWith('open_step_download', { lang: 'de' });
  });
});
