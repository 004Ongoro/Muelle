//! Muelle settings: a separate process that exists only while its window is
//! open. It edits the configuration file and tells the dock to reload it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::env;

use muelle_config::Config;
use muelle_shell::{autostart, dock_link};

/// The dock owns the pinned and recent apps; this app never overwrites them
/// with a stale copy.
fn merged_with_disk(config: Config) -> Config {
    match muelle_config::load(&muelle_config::default_path()) {
        Ok(Some(disk)) => Config {
            pinned: disk.pinned,
            recent: disk.recent,
            ..config
        },
        _ => config,
    }
}

#[tauri::command]
fn get_config() -> Config {
    muelle_config::load(&muelle_config::default_path())
        .ok()
        .flatten()
        .unwrap_or_default()
}

#[tauri::command]
fn set_config(config: Config) -> Result<Config, String> {
    let config = merged_with_disk(config.sanitized());
    muelle_config::save(&muelle_config::default_path(), &config)
        .map_err(|error| error.to_string())?;
    dock_link::notify_config_changed();
    Ok(config)
}

#[tauri::command]
fn get_autostart() -> bool {
    autostart::is_enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    let dock = env::current_exe()
        .map_err(|error| error.to_string())?
        .with_file_name("muelle.exe");
    let target = enabled.then(|| dock.to_string_lossy().into_owned());
    autostart::set(target.as_deref()).map_err(|error| error.to_string())
}

fn main() {
    muelle_config::migrate_legacy_folder();
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            get_autostart,
            set_autostart
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("muelle-settings: {error}");
    }
}
