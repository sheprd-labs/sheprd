//! The machines this Mac holds, by name, for menus that list them as they
//! draw and cannot read the disk then. It is read again when such a menu
//! opens and when Settings or an install reads the folders.

use super::Name;
use gpui::App;
use std::cell::RefCell;

thread_local! {
    static KNOWN: RefCell<Vec<Name>> = const { RefCell::new(Vec::new()) };
}

/// The machines last read, sorted by name.
pub(crate) fn known() -> Vec<Name> {
    KNOWN.with_borrow(Clone::clone)
}

/// Keeps `names` as the machines this Mac holds, and redraws if they changed.
pub(crate) fn remember(names: Vec<Name>, cx: &mut App) {
    let changed = KNOWN.with_borrow_mut(|known| {
        let changed = *known != names;
        *known = names;
        changed
    });
    if changed {
        cx.refresh_windows();
    }
}

/// Reads the machines' folders again, off the UI thread.
pub(crate) fn refresh(cx: &mut App) {
    // Tests list what they remember, never this Mac's own machines.
    if cfg!(test) {
        return;
    }
    let read = cx.background_executor().spawn(async { super::list() });
    cx.spawn(async move |cx| {
        if let Ok(listings) = read.await {
            let names = listings.into_iter().map(|listing| listing.name).collect();
            cx.update(|cx| remember(names, cx));
        }
    })
    .detach();
}
