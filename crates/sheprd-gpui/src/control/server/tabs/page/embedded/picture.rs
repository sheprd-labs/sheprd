//! WebKit captures a tab at its viewport size, including when it is hidden.
//! The resulting PNG is written atomically off the UI thread.
use super::{inspection_window, page_tab};
use crate::control::{
    protocol::{Caller, ErrorCode, Response, tabs::PAGE_WAIT_SECONDS},
    server::{Watches, socket},
};
use gpui::App;
use std::{
    cell::RefCell,
    io::Write as _,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

/// Where a screenshot request is.
enum Phase {
    /// Waiting for the page to finish loading.
    Loading,
    /// WebKit was asked; its TIFF, or `None` for no picture, lands here.
    Capturing(Rc<RefCell<Option<Option<Vec<u8>>>>>),
    /// The background thread writes the PNG and reports here.
    Writing(Arc<Mutex<Option<crate::Result<()>>>>),
}

/// An absolute, bounded, printable path, which the caller made absolute in
/// its own working directory.
fn screenshot_path(path: &str) -> Option<PathBuf> {
    let valid = !path.is_empty()
        && path.len() <= 4096
        && !path.chars().any(char::is_control)
        && Path::new(path).is_absolute()
        && Path::new(path).file_name().is_some();
    valid.then(|| PathBuf::from(path))
}

/// Turns a capture into a PNG and replaces `path` with it. Blocking: runs
/// on the background executor.
pub(super) fn save_png(tiff: &[u8], path: &Path) -> crate::Result<()> {
    let png = crate::browser::page_png(tiff).ok_or(crate::Error::ScreenshotImage)?;
    let failed = |source| crate::Error::ScreenshotWrite {
        path: path.to_owned(),
        source,
    };
    let parent = path
        .parent()
        .ok_or_else(|| failed(std::io::ErrorKind::InvalidInput.into()))?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(failed)?;
    file.write_all(&png).map_err(failed)?;
    file.persist(path).map_err(|error| failed(error.error))?;
    Ok(())
}

pub(in crate::control::server::tabs) fn screenshot(
    caller: Caller,
    id: u64,
    path: String,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let Some(file) = screenshot_path(&path) else {
        return incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "The screenshot needs an absolute file path without control characters",
        ));
    };
    let tab = match page_tab(id, cx) {
        Ok(tab) => tab,
        Err(response) => return incoming.respond(response),
    };
    let handle = match inspection_window(&caller, &tab, cx) {
        Ok(handle) => handle,
        Err(response) => return incoming.respond(response),
    };
    let tab_id = tab.id;
    let mut phase = Phase::Loading;
    watches.add(
        incoming,
        Duration::from_secs(PAGE_WAIT_SECONDS),
        move |cx| match &phase {
            Phase::Loading => {
                let landed = Rc::new(RefCell::new(None));
                let slot = landed.clone();
                let asked = handle
                    .read_with(cx, |view, _| {
                        view.page_screenshot(tab_id, move |tiff| *slot.borrow_mut() = Some(tiff))
                    })
                    .unwrap_or(false);
                if asked {
                    phase = Phase::Capturing(landed);
                }
                None
            }
            Phase::Capturing(landed) => {
                let Some(tiff) = landed.borrow_mut().take()? else {
                    return Some(Response::error(
                        ErrorCode::Failed,
                        "WebKit took no picture of the page",
                    ));
                };
                let written: Arc<Mutex<Option<crate::Result<()>>>> = Arc::default();
                let report = written.clone();
                let file = file.clone();
                cx.background_executor()
                    .spawn(async move {
                        let saved = save_png(&tiff, &file);
                        *report.lock().unwrap_or_else(PoisonError::into_inner) = Some(saved);
                    })
                    .detach();
                phase = Phase::Writing(written);
                None
            }
            Phase::Writing(written) => {
                let saved = written
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()?;
                Some(match saved {
                    Ok(()) => Response::done(format!(
                        "Saved a screenshot of tab {id} to {}",
                        file.display()
                    )),
                    Err(error) => Response::error(ErrorCode::Failed, error.to_string()),
                })
            }
        },
        Response::error(
            ErrorCode::Timeout,
            format!("Tab {id}'s page did not finish loading in time"),
        ),
        cx,
    );
}

#[cfg(test)]
mod tests;
