// One tauri-driver per test file. The driver inherits the environment of the
// process that starts it, and the app inherits it from the driver - so
// starting a driver per file is how each file gets its own HOME/XDG_*.
import { spawn, spawnSync, type ChildProcess } from 'node:child_process';
import { createConnection } from 'node:net';
import { IS_WINDOWS } from './run-env.js';

export const DRIVER_PORT = 4444;

let driver: ChildProcess | null = null;

async function portOpen(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = createConnection({ port, host: '127.0.0.1' });
    socket.once('connect', () => { socket.destroy(); resolve(true); });
    socket.once('error', () => resolve(false));
  });
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

export async function startDriver(env: Record<string, string>): Promise<void> {
  const args = ['--port', String(DRIVER_PORT)];
  if (process.env.MFK_E2E_NATIVE_DRIVER) args.push('--native-driver', process.env.MFK_E2E_NATIVE_DRIVER);
  driver = spawn(process.env.MFK_E2E_TAURI_DRIVER || 'tauri-driver', args, {
    env: { ...process.env, ...env },
    stdio: ['ignore', 'inherit', 'inherit'],
  });
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    if (await portOpen(DRIVER_PORT)) return;
    await sleep(200);
  }
  throw new Error('tauri-driver did not open its port');
}

export async function stopDriver(): Promise<void> {
  if (!driver) return;
  const child = driver;
  driver = null;
  child.kill();
  await Promise.race([new Promise((r) => child.once('exit', r)), sleep(5000)]);
  // Wait for the port to close so the next file can reuse it.
  for (let i = 0; i < 50 && (await portOpen(DRIVER_PORT)); i++) await sleep(100);
}

/** True while any process runs the app binary (the single-instance lock is held until it is gone). */
export function appStillRunning(appPath: string): boolean {
  if (IS_WINDOWS) {
    const exe = appPath.split(/[\\/]/).pop()!;
    const out = spawnSync('tasklist', ['/FI', `IMAGENAME eq ${exe}`], { encoding: 'utf8' }).stdout ?? '';
    return out.toLowerCase().includes(exe.toLowerCase());
  }
  // The AppImage wrapper execs into the real binary; comm is cut to 15 characters (mf-katalog-mana).
  return spawnSync('pgrep', ['-x', 'mf-katalog-mana'], { encoding: 'utf8' }).status === 0;
}

export async function waitForAppGone(appPath: string, ms = 20000): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (!appStillRunning(appPath)) return;
    await sleep(250);
  }
  // Last resort so one hung instance cannot fail every later file.
  if (IS_WINDOWS) spawnSync('taskkill', ['/F', '/IM', appPath.split(/[\\/]/).pop()!]);
  else spawnSync('pkill', ['-9', '-x', 'mf-katalog-mana']);
  await sleep(500);
}
