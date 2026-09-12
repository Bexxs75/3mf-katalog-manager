// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMA-BUF-Renderer scheitert auf manchen NVIDIA-Treibern beim
    // Anlegen des GBM-Buffers ("Failed to create GBM buffer") und die App
    // zeigt nur einen weissen Bildschirm. Muss vor jeglicher GTK/WebKit-
    // Initialisierung gesetzt werden, daher ganz am Anfang von main().
    #[cfg(target_os = "linux")]
    // SAFETY: single-threaded, allererste Zeile von main() - kein anderer
    // Thread liest/schreibt zu diesem Zeitpunkt Umgebungsvariablen.
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    mf_katalog_manager_lib::run()
}
