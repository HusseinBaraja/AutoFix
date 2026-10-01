//! Preserve every available format before replacing clipboard contents.
//! Unsupported owner-managed formats refuse the path before EmptyClipboard.
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};

#[derive(Default)]
struct ClipboardPolicy {
    failed_apps: HashSet<String>,
}
impl ClipboardPolicy {
    /// Refuse clipboard mutation for apps with a prior restoration failure this run.
    fn allowed(&self, app: &str) -> bool {
        !self.failed_apps.contains(&app.to_ascii_lowercase())
    }

    /// Remember failure by case-insensitive process name without retaining target text.
    #[cfg(any(windows, test))]
    fn failed(&mut self, app: &str) {
        self.failed_apps.insert(app.to_ascii_lowercase());
    }
}

/// Share memory-only fallback preferences across correction and undo.
fn policy() -> &'static Mutex<ClipboardPolicy> {
    static POLICY: OnceLock<Mutex<ClipboardPolicy>> = OnceLock::new();
    POLICY.get_or_init(|| Mutex::new(ClipboardPolicy::default()))
}

/// A poisoned policy lock fails closed to the non-clipboard fallback.
pub(super) fn allowed_for(app: &str) -> bool {
    policy().lock().is_ok_and(|policy| policy.allowed(app))
}

/// Record per-app fallback and emit metadata without clipboard or document contents.
#[cfg(windows)]
pub(super) fn restore_failed(app: &str, process_id: u32) {
    if let Ok(mut policy) = policy().lock() {
        policy.failed(app);
    }
    // No document, replacement, clipboard contents, or window title in diagnostics.
    tracing::warn!(
        process_id,
        method = "clipboard",
        "clipboard restoration failed; prefer fallback for this app"
    );
}

/// Restoration runs even after failed paste, before target verification.
#[cfg(any(windows, test))]
pub(super) fn paste_and_restore(
    paste: impl FnOnce() -> Result<(), String>,
    restore: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let pasted = paste();
    let restored = restore();
    match (pasted, restored) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(reason), Ok(())) | (Ok(()), Err(reason)) => Err(reason),
        (Err(reason), Err(restore)) => Err(format!("{reason}; {restore}")),
    }
}

/// Saved originals are never transferred. Preallocated copies make the first restore
/// allocation-free; retries can duplicate originals without retaining temporary text.
#[cfg(any(windows, test))]
fn restore_snapshot<T>(
    saved: &[T],
    prepared: &mut Option<Vec<T>>,
    mut duplicate: impl FnMut(&T) -> Result<T, String>,
    clear: impl FnOnce() -> Result<(), String>,
    mut publish: impl FnMut(&mut T) -> Result<(), String>,
) -> Result<(), String> {
    let copies = prepared.take().map(Ok).unwrap_or_else(|| {
        saved
            .iter()
            .map(&mut duplicate)
            .collect::<Result<Vec<_>, _>>()
    });
    if let Err(reason) = clear() {
        *prepared = copies.ok();
        return Err(reason);
    }
    let mut copies = copies?;
    let mut failure = None;
    for copy in &mut copies {
        if let Err(reason) = publish(copy) {
            failure = Some(reason);
        }
    }
    failure.map_or(Ok(()), Err)
}

/// Allow only published formats and their known Windows-synthesized equivalents.
#[cfg(any(windows, test))]
fn owned_format_or_synthesis(known: &[u32], format: u32) -> bool {
    known.contains(&format)
        || match format {
            1 | 7 | 13 | 16 => known.iter().any(|format| matches!(format, 1 | 7 | 13)),
            2 | 8 | 17 => known.iter().any(|format| matches!(format, 2 | 8 | 17)),
            3 | 14 => known.iter().any(|format| matches!(format, 3 | 14)),
            _ => false,
        }
}

#[cfg(test)]
mod tests;

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub(in crate::background::replacement) use windows::{paste, supports_paste, ClipboardTransaction};

/// Finish memory-only recovery before the engine returns to its host.
#[cfg(windows)]
pub(super) fn shutdown() {
    windows::shutdown();
}
