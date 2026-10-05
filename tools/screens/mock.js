// Replaces the Tauri bridge in the browser: read commands return the answers
// recorded with the real Rust code (tools/screens/fixtures), write commands are
// kept in memory or just acknowledged. Modelled on @tauri-apps/api/mocks
// (mockIPC + mockWindows). __FIXTURE__ and __PREFS__ are filled in by the runner.
(function () {
  const FX = __FIXTURE__;
  const PREFS = __PREFS__;
  try {
    for (const [k, v] of Object.entries(PREFS)) localStorage.setItem(k, v);
  } catch (e) {}

  const clone = (v) => (v === undefined ? null : JSON.parse(JSON.stringify(v)));
  const b64ToBuf = (b64) => {
    const bin = atob(b64);
    const out = new Uint8Array(bin.length);
    for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
    return out.buffer;
  };
  const snapshots = window.__demoSnapshots = Object.assign({}, __SNAPSHOTS__);
  const withSnapshots = (files) =>
    files.map((f) => (snapshots[f.id] ? { ...f, renderSnapshotImage: snapshots[f.id] } : f));
  window.__demoUnknown = [];

  const callbacks = new Map();
  const listeners = new Map();
  function registerCallback(cb, once = false) {
    const id = window.crypto.getRandomValues(new Uint32Array(1))[0];
    callbacks.set(id, (data) => {
      if (once) callbacks.delete(id);
      return cb && cb(data);
    });
    return id;
  }

  async function invoke(cmd, args = {}) {
    switch (cmd) {
      case 'plugin:event|listen':
        if (!listeners.has(args.event)) listeners.set(args.event, []);
        listeners.get(args.event).push(args.handler);
        return args.handler;
      case 'plugin:event|unlisten':
      case 'plugin:event|emit':
        return null;
      case 'list_files':
        return withSnapshots(clone(FX.list_files));
      case 'list_file_summaries':
        return withSnapshots(clone(FX.list_file_summaries));
      case 'list_files_by_ids':
        return withSnapshots(clone(FX.list_files.filter((f) => (args.ids || []).includes(f.id))));
      case 'list_collection_files':
        return withSnapshots(clone(FX['list_collection_files:' + args.collectionId] || []));
      case 'list_print_log_entries':
        return clone(FX['list_print_log_entries:' + args.fileId] || []);
      case 'get_model_geometry':
        if (window.__demoFailGeometry) throw { message: 'not found', expected: true, code: 'notFound' };
        return b64ToBuf(FX.__geometry[args.fileId]);
      case 'set_render_snapshot':
        snapshots[args.fileId] = 'data:image/png;base64,' + args.imageBase64;
        return null;
      case 'check_for_update':
        return { currentVersion: FX.get_app_version, latestVersion: FX.get_app_version, updateAvailable: false, releaseUrl: '' };
      case 'import_files':
      case 'import_dropped':
        return { imported: [], duplicateCount: 0, pendingArchives: (FX.inspect_archives || []).map((a) => a.path) };
      case 'archive_target_conflicts':
        return (args.folderNames || []).map(() => false);
      case 'scan_installed_slicers':
        return clone(FX.list_registered_slicers);
      case 'list_file_images':
        return (args.ids || []).map((id) => {
          const f = FX.list_files.find((x) => x.id === id) || {};
          return { id, thumbnailImage: f.thumbnailImage ?? null, renderSnapshotImage: snapshots[id] || f.renderSnapshotImage || null, customImage: f.customImage ?? null };
        });
      case 'start_import':
      case 'start_dropped_import':
      case 'start_adopt_import':
        return { jobId: 'demo-job' };
      case 'get_import_state':
        return window.__demoImportProgress || null;
      case 'get_import_result':
        return window.__demoImportResult || null;
      case 'cancel_import':
        return null;
      case 'check_filament': {
        const st = { '1': 'ok', '2': 'swap', '3': 'short', '5': 'unknown', '6': 'no_data' };
        return (args.ids || args.fileIds || []).map((id) => {
          const status = st[id] || 'ok';
          return { fileId: id, status, needs: status === 'short' ? [{ filamentType: 'PETG', color: null, neededG: 137, status: 'short', missingG: 48.2, spools: [], possible: [] }] : [] };
        });
      }
      case 'get_last_printer_for_file':
        return ['1', '5'].includes(String(args.fileId)) ? { printerName: 'Sovol SV08', endedAt: Math.floor(Date.now() / 1000) - 3 * 86400 } : null;
      case 'has_step_preview':
        return false;
      case 'save_window_theme':
      case 'frontend_ready':
      case 'mark_file_viewed':
      case 'register_existing_catalog_base_dir':
        return null;
    }
    if (Object.prototype.hasOwnProperty.call(FX, cmd)) return clone(FX[cmd]);
    window.__demoUnknown.push(cmd);
    return null;
  }

  window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = window.__TAURI_EVENT_PLUGIN_INTERNALS__ || {};
  window.__TAURI_INTERNALS__.invoke = invoke;
  window.__TAURI_INTERNALS__.transformCallback = registerCallback;
  window.__TAURI_INTERNALS__.unregisterCallback = (id) => callbacks.delete(id);
  window.__TAURI_INTERNALS__.runCallback = (id, data) => callbacks.get(id) && callbacks.get(id)(data);
  window.__TAURI_INTERNALS__.callbacks = callbacks;
  window.__TAURI_INTERNALS__.metadata = {
    currentWindow: { label: 'main' },
    currentWebview: { windowLabel: 'main', label: 'main' },
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener = (_e, id) => callbacks.delete(id);
  // For the recipes: send any Tauri event to the UI
  window.__demoEmit = (event, payload) => {
    for (const h of listeners.get(event) || []) {
      const cb = callbacks.get(h);
      if (cb) cb({ event, id: h, payload });
    }
  };
  // For the recipes: trigger the archive dialog with a file drop
  window.__demoEmitDrop = (paths) => {
    for (const ev of ['tauri://drag-drop']) {
      for (const h of listeners.get(ev) || []) {
        const cb = callbacks.get(h);
        if (cb) cb({ event: ev, id: h, payload: { paths, position: { x: 700, y: 400 } } });
      }
    }
  };
})();
