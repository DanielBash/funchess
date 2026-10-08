// UI language: Russian by default, English on the EN/RU toggle (saved in settings).
use std::sync::atomic::{AtomicBool, Ordering};

static RU: AtomicBool = AtomicBool::new(true);

pub fn set_ru(on: bool) {
    RU.store(on, Ordering::Relaxed);
}

pub fn ru() -> bool {
    RU.load(Ordering::Relaxed)
}

/// Picks the string for the current language.
pub fn tr<'a>(en: &'a str, ru_: &'a str) -> &'a str {
    if ru() { ru_ } else { en }
}
