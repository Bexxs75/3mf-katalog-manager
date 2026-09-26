// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMA-BUF renderer fails on some NVIDIA drivers when creating the
    // GBM buffer ("Failed to create GBM buffer") and the app only shows a white
    // screen. Must be set before any GTK/WebKit initialization, hence at the very
    // start of main().
    #[cfg(target_os = "linux")]
    // SAFETY: single-threaded, the very first line of main() - no other thread
    // reads or writes environment variables at this point.
    unsafe {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    mf_katalog_manager_lib::run()
}
