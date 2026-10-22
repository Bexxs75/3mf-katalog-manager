import { render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
import { LanguageProvider } from '../i18n/LanguageContext';
import { BulkDeleteFailureBanner } from './BulkDeleteFailureBanner';
import { ImportSummaryBanner } from './ImportSummaryBanner';
import { NoticeStack } from './NoticeStack';

vi.mock('@tauri-apps/plugin-log', () => ({ error: vi.fn(() => Promise.resolve()), info: vi.fn(() => Promise.resolve()) }));

beforeEach(() => { localStorage.setItem('3mf-katalog-language', 'de'); });

it('stacks both bottom-right banners in one container instead of positioning each', () => {
  render(
    <LanguageProvider>
      <NoticeStack>
        <ImportSummaryBanner imported={2} duplicates={0} skipped={[]} otherFilesExtracted={0} jobHasProblems={false} onClose={vi.fn()} />
        <BulkDeleteFailureBanner names={['a']} onClose={vi.fn()} />
      </NoticeStack>
    </LanguageProvider>,
  );
  const stack = screen.getByTestId('notice-stack');
  const alert = screen.getByRole('alert');
  expect(stack).toContainElement(alert);
  expect(stack.className).toContain('fixed');
  expect(stack.className).toContain('pointer-events-none');
  expect(stack.children).toHaveLength(2);
  for (const banner of Array.from(stack.children)) {
    expect(banner.className).not.toMatch(/\bfixed\b/);
    expect(banner.className).toContain('pointer-events-auto');
  }
});
