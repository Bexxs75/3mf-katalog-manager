import source from '../../crates/katalog-server/src/invoke.rs?raw';
import writeSource from '../../crates/katalog-server/src/write.rs?raw';
import { afterEach, expect, it, vi } from 'vitest';
import { isReadOnly, WEB_READ_COMMANDS, WEB_WRITE_COMMANDS } from './readOnly';
afterEach(() => { vi.unstubAllEnvs(); vi.unstubAllGlobals(); });
it('is enabled only for the web build', () => {
  vi.stubEnv('VITE_TARGET', 'desktop'); expect(isReadOnly()).toBe(false);
  vi.stubEnv('VITE_TARGET', 'web'); expect(isReadOnly()).toBe(true);
});
it.each([[source, 'READ_COMMANDS', WEB_READ_COMMANDS], [writeSource, 'WRITE_COMMANDS', WEB_WRITE_COMMANDS]])('matches the exported Rust constant', (rust, name, list) => {
  const constant = (rust as string).split(`pub const ${name}: &[&str] = &[`)[1]?.split('];')[0];
  expect(constant).toBeDefined();
  const commands = [...constant.matchAll(/"([a-z_]+)"/g)].map(match => match[1]);
  expect([...list].sort()).toEqual(commands.sort());
});
it('rejects unavailable commands locally with an expected error', async () => {
  vi.stubEnv('VITE_TARGET', 'web'); vi.resetModules();
  const fetch = vi.fn(); vi.stubGlobal('fetch', fetch);
  const { invoke } = await import('./transport');
  for (const command of ['set_favorite', 'mark_file_viewed', 'set_render_snapshot', 'open_in_file_manager', 'unknown']) {
    await expect(invoke(command)).rejects.toMatchObject({ expected: true, message: 'Im Web-Modus nicht verfügbar' });
  }
  expect(fetch).not.toHaveBeenCalled();
});
it.each([
  ['de', 'Im Web-Modus nicht verfügbar'], ['en', 'Unavailable in web mode'],
  ['es', 'No disponible en modo web'], ['fr', 'Indisponible en mode web'],
])('localizes the defensive error in %s', async (language, message) => {
  vi.stubEnv('VITE_TARGET', 'web'); vi.resetModules();
  document.documentElement.lang = language;
  const fetch = vi.fn(); vi.stubGlobal('fetch', fetch);
  await expect((await import('./transport')).invoke('delete_file')).rejects.toEqual({ expected: true, message });
  expect(fetch).not.toHaveBeenCalled();
  document.documentElement.lang = 'de';
});
