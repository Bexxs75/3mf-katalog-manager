import { expect, it } from 'vitest';
import pkg from '../../package.json';
import tauriConf from '../../src-tauri/tauri.conf.json';
import tauriCargo from '../../src-tauri/Cargo.toml?raw';
import coreCargo from '../../crates/katalog-core/Cargo.toml?raw';
import serverCargo from '../../crates/katalog-server/Cargo.toml?raw';

// The web server reports its own crate version as the app version ("get_app_version"), so every crate must carry the app version.
// `set-version` (release process) has to touch all of these files; this test keeps the crates from falling behind again.
const cargoVersion = (source: string) => /^version = "([^"]+)"/m.exec(source)?.[1];

it('shows one version in the package, the Tauri config and every crate', () => {
  expect(cargoVersion(tauriCargo as string)).toBe(pkg.version);
  expect(tauriConf.version).toBe(pkg.version);
  expect(cargoVersion(coreCargo as string)).toBe(pkg.version);
  expect(cargoVersion(serverCargo as string)).toBe(pkg.version);
});
