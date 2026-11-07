import { render, screen } from '@testing-library/react';
import { beforeEach, expect, it, vi } from 'vitest';
vi.hoisted(() => vi.stubEnv('VITE_TARGET', 'web'));
import { LanguageProvider } from '../i18n/LanguageContext';
import { ThingiverseSettings } from './ThingiverseSettings';
import * as api from '../lib/api/thingiverse';
import { de } from '../i18n/de';

vi.mock('../lib/api/thingiverse');

beforeEach(() => localStorage.setItem('3mf-katalog-language', 'de'));

it('renders nothing in the web build and never asks the backend', async () => {
  vi.mocked(api.getThingiverseSettings).mockResolvedValue({ enabled: true, hasToken: true, tokenMask: '••••1234', usageToday: 0, dailyLimit: 1000 });
  const { container } = render(<LanguageProvider><ThingiverseSettings onEnabledChange={() => {}} /></LanguageProvider>);
  await new Promise(resolve => setTimeout(resolve, 20));
  expect(container).toBeEmptyDOMElement();
  expect(screen.queryByText(de.tvSwitchLabel)).toBeNull();
  expect(api.getThingiverseSettings).not.toHaveBeenCalled();
});
