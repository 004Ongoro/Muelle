//! User-facing dock text in Spanish and English. The language follows the
//! Windows display language; Settings will be able to override it (phase 7).
//! `&` marks the menu access key.

use std::sync::atomic::{AtomicU8, Ordering};

use muelle_config::Language;
use muelle_shell::system::ui_language_is_spanish;

pub struct Strings {
    pub open: &'static str,
    pub new_window: &'static str,
    pub keep_in_dock: &'static str,
    pub show_in_explorer: &'static str,
    pub quit_app: &'static str,
    pub empty_bin: &'static str,
    pub turn_hiding_on: &'static str,
    pub turn_hiding_off: &'static str,
    pub quit_dock: &'static str,
    pub start: &'static str,
    pub remove: &'static str,
    pub close_window: &'static str,
    pub settings: &'static str,
}

const ES: Strings = Strings {
    open: "&Abrir",
    new_window: "&Nueva ventana",
    keep_in_dock: "&Mantener en el Dock",
    show_in_explorer: "Mostrar en el &Explorador",
    quit_app: "&Salir",
    empty_bin: "&Vaciar papelera",
    turn_hiding_on: "Activar &ocultación automática",
    turn_hiding_off: "Desactivar &ocultación automática",
    quit_dock: "Salir de &Muelle",
    start: "Inicio",
    remove: "Quitar",
    close_window: "&Cerrar",
    settings: "&Ajustes de Muelle…",
};

const EN: Strings = Strings {
    open: "&Open",
    new_window: "&New Window",
    keep_in_dock: "&Keep in Dock",
    show_in_explorer: "Show in &Explorer",
    quit_app: "&Quit",
    empty_bin: "&Empty Recycle Bin",
    turn_hiding_on: "Turn Hiding &On",
    turn_hiding_off: "Turn Hiding &Off",
    quit_dock: "Quit &Muelle",
    start: "Start",
    remove: "Remove",
    close_window: "&Close",
    settings: "Muelle &Settings…",
};

/// 0 = not chosen yet (follow Windows), 1 = Spanish, 2 = English.
static LANGUAGE: AtomicU8 = AtomicU8::new(0);

pub fn set_language(language: Language) {
    let spanish = match language {
        Language::Auto => ui_language_is_spanish(),
        Language::Es => true,
        Language::En => false,
    };
    LANGUAGE.store(if spanish { 1 } else { 2 }, Ordering::Relaxed);
}

pub fn text() -> &'static Strings {
    match LANGUAGE.load(Ordering::Relaxed) {
        0 => {
            set_language(Language::Auto);
            text()
        }
        1 => &ES,
        _ => &EN,
    }
}
