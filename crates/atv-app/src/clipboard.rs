//! The desktop clipboard (arboard) for Copy ride link, Copy invite link and
//! pasting into the Host/Join fields; the browser build uses
//! `navigator.clipboard` and its own text inputs instead.

thread_local! {
    /// One clipboard for the app's life: on X11 the copied text is served
    /// by the owner, so it must outlive the copy.
    static CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

fn with<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Option<T>) -> Option<T> {
    CLIPBOARD.with_borrow_mut(|clipboard| {
        if clipboard.is_none() {
            *clipboard = arboard::Clipboard::new().ok();
        }
        f(clipboard.as_mut()?)
    })
}

/// Puts `text` on the clipboard; false when the system refused it.
pub fn copy(text: &str) -> bool {
    with(|c| c.set_text(text.to_owned()).ok()).is_some()
}

/// The clipboard's text, if any.
pub fn paste() -> Option<String> {
    with(|c| c.get_text().ok())
}
