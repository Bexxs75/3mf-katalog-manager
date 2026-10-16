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

it('keeps manager scope explicit and printer networking unavailable', () => {
  expect(WEB_WRITE_COMMANDS).toHaveLength(48);
  expect(WEB_READ_COMMANDS).toContain('get_filament_spool_image');
  for (const command of ['restock_filament_spool', 'consume_resin', 'set_printer_link_enabled', 'test_printer_connection', 'remove_printer_connection', 'sync_printers_now', 'pick_and_read_image']) {
    expect([...WEB_READ_COMMANDS, ...WEB_WRITE_COMMANDS]).not.toContain(command);
  }
});

it('keeps file operations in the write audit allowlist and dispatches before write_invoke', async () => {
  const fileops = writeSource.split('pub const FILE_OPS_COMMANDS: &[&str] = &[')[1].split('];')[0];
  for (const match of fileops.matchAll(/"([a-z_]+)"/g)) expect(WEB_WRITE_COMMANDS).toContain(match[1]);
  const web = (await import('../../crates/katalog-server/src/web.rs?raw')).default;
  expect(web.indexOf('return fileops::invoke')).toBeLessThan(web.indexOf('return write_invoke'));
});
