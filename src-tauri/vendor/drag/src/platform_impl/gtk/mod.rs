// Copyright 2023-2023 CrabNebula Ltd.
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

// Locally modified: encoded file URIs and completion-time handler cleanup.
// See ../../../VENDORING.md for provenance and upstream restoration.

use crate::{CursorPosition, DragItem, DragResult, Error, Image, Options};
use gdkx11::{
    gdk,
    glib::{ObjectExt, Propagation, SignalHandlerId},
};
use gtk::{
    gdk_pixbuf,
    prelude::{
        DeviceExt, DragContextExtManual, PixbufLoaderExt, SeatExt, WidgetExt, WidgetExtManual,
    },
};
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
};

pub fn start_drag<F: Fn(DragResult, CursorPosition) + Send + 'static>(
    window: &gtk::ApplicationWindow,
    item: DragItem,
    image: Image,
    on_drop_callback: F,
    options: Options,
) -> crate::Result<()> {
    log::debug!("Starting drag operation with mode: {:?}", options.mode);
    let handler_ids: Arc<Mutex<Vec<SignalHandlerId>>> = Arc::new(Mutex::new(vec![]));
    // Catalog originals must never be offered for a destructive move.
    let drag_action = gdk::DragAction::COPY;

    log::debug!("Setting drag source with action: {:?}", drag_action);
    window.drag_source_set(gdk::ModifierType::BUTTON1_MASK, &[], drag_action);

    match item {
        DragItem::Files(paths) => {
            log::debug!("Setting up file drag with {} paths", paths.len());
            // Explicit targets avoid GTK adding portal formats that we do not serve.
            let targets = gtk::TargetList::new(&[
                gtk::TargetEntry::new("text/uri-list", gtk::TargetFlags::empty(), 0),
            ]);
            window.drag_source_set_target_list(Some(&targets));
            handler_ids
                .lock()
                .unwrap()
                .push(window.connect_drag_data_get(move |_, _, data, _, _| {
                    log::debug!("Preparing URIs for drag data");
                    let uris: Vec<String> = paths
                        .iter()
                        .filter_map(|path| file_uri(path).ok())
                        .collect();
                    let uris: Vec<&str> = uris.iter().map(|s| s.as_str()).collect();
                    log::debug!("Setting URIs: {:?}", uris);
                    data.set_uris(&uris);
                }));
        }
        DragItem::Data { .. } => {
            // Currently leaving it as is as we can utilize it as a dummy dragging feature
            // on_drop_callback(DragResult::Cancel, get_cursor_position(window).unwrap());
            // return Ok(());
        }
    }

    if let Some(target_list) = &window.drag_source_get_target_list() {
        log::debug!("Got target list, initiating drag");
        if let Some(drag_context) = window.drag_begin_with_coordinates(
            target_list,
            drag_action,
            gdk::ffi::GDK_BUTTON1_MASK as i32,
            None,
            -1,
            -1,
        ) {
            log::debug!("Drag context created successfully");
            let callback = Rc::new(on_drop_callback);
            let dropped = Rc::new(Cell::new(false));
            let reported = Rc::new(Cell::new(false));
            on_drop_failed(callback.clone(), window, &handler_ids, &options, reported.clone());
            let performed = dropped.clone();
            let performed_id = drag_context.connect_drop_performed(move |_, _| {
                // Releasing the button precedes late targets' drag-data-get requests.
                performed.set(true);
            });
            let handlers = handler_ids.clone();
            let context = drag_context.clone();
            let performed_id = std::cell::RefCell::new(Some(performed_id));
            handler_ids.lock().unwrap().push(window.connect_drag_end(move |window, _| {
                if !reported.replace(true) {
                    callback(
                        if dropped.get() { DragResult::Dropped } else { DragResult::Cancel },
                        get_cursor_position(window).unwrap_or(CursorPosition { x: 0, y: 0 }),
                    );
                }
                if let Some(id) = performed_id.borrow_mut().take() { context.disconnect(id); }
                // Only drag-end guarantees that the destination finished requesting data.
                cleanup_signal_handlers(&handlers, window);
            }));

            log::debug!("Setting up drag icon");
            let icon_pixbuf: Option<gdk_pixbuf::Pixbuf> = match &image {
                Image::Raw(data) => image_binary_to_pixbuf(data),
                Image::File(path) => match std::fs::read(path) {
                    Ok(bytes) => image_binary_to_pixbuf(&bytes),
                    Err(_) => None,
                },
            };
            if let Some(icon) = icon_pixbuf {
                drag_context.drag_set_icon_pixbuf(&icon, 0, 0);
            }

            Ok(())
        } else {
            cleanup_signal_handlers(&handler_ids, window);
            Err(crate::Error::FailedToStartDrag)
        }
    } else {
        cleanup_signal_handlers(&handler_ids, window);
        Err(crate::Error::EmptyTargetList)
    }
}

fn image_binary_to_pixbuf(data: &[u8]) -> Option<gdk_pixbuf::Pixbuf> {
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader
        .write(data)
        .and_then(|_| loader.close())
        .map_err(|_| ())
        .and_then(|_| loader.pixbuf().ok_or(()))
        .ok()
}

fn clear_signal_handlers(window: &gtk::ApplicationWindow, handler_ids: &mut Vec<SignalHandlerId>) {
    for handler_id in handler_ids.drain(..) {
        window.disconnect(handler_id);
    }
}

fn on_drop_failed<F: Fn(DragResult, CursorPosition) + Send + 'static>(
    callback: Rc<F>,
    window: &gtk::ApplicationWindow,
    handler_ids: &Arc<Mutex<Vec<SignalHandlerId>>>,
    options: &Options,
    reported: Rc<Cell<bool>>,
) {
    log::debug!("Setting up drop failed handler");
    let window_clone = window.clone();

    let skip_animatation_on_cancel_or_failure = options.skip_animatation_on_cancel_or_failure;

    handler_ids
        .lock()
        .unwrap()
        .push(window.connect_drag_failed(move |_, _, _drag_result| {
            log::debug!("Drag failed or cancelled");
            // Failure and drag-end both fire on cancellation; report only once.
            if !reported.replace(true) {
                callback(
                    DragResult::Cancel,
                    get_cursor_position(&window_clone).unwrap_or(CursorPosition { x: 0, y: 0 }),
                );
            }
            if skip_animatation_on_cancel_or_failure {
                Propagation::Stop
            } else {
                Propagation::Proceed
            }
        }));
}

fn cleanup_signal_handlers(
    handler_ids: &Arc<Mutex<Vec<SignalHandlerId>>>,
    window: &gtk::ApplicationWindow,
) {
    log::debug!("Cleaning up signal handlers");
    let handler_ids = &mut handler_ids.lock().unwrap();
    clear_signal_handlers(window, handler_ids);
    window.drag_source_unset();
    log::debug!("Signal handlers cleaned up");
}

fn get_cursor_position(window: &gtk::ApplicationWindow) -> Result<CursorPosition, Error> {
    if let Some(cursor) = window
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer())
    {
        let (_, x, y) = cursor.position();
        Ok(CursorPosition { x, y })
    } else {
        Err(Error::FailedToGetCursorPosition)
    }
}

// GLib encodes filesystem bytes, including percent signs and non-ASCII names.
fn file_uri(path: &std::path::Path) -> Result<String, gdkx11::glib::Error> {
    gdkx11::glib::filename_to_uri(path, None).map(|uri| uri.to_string())
}

#[cfg(test)]
mod tests {
    use super::file_uri;
    #[test]
    fn file_uri_encodes_special_characters() {
        let uri = file_uri(std::path::Path::new("/tmp/Würfel 20 (Test)%#.3mf")).unwrap();
        assert_eq!(uri, "file:///tmp/W%C3%BCrfel%2020%20(Test)%25%23.3mf");
    }
    #[test]
    fn file_uri_rejects_relative_paths() {
        assert!(file_uri(std::path::Path::new("relative.3mf")).is_err());
    }
}
