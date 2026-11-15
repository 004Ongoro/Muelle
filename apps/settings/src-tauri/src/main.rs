//! Muelle settings: a separate process that exists only while its window is
//! open. It edits the configuration file and tells the dock to reload it.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, io, path::PathBuf, process::Command, sync::Mutex};

use muelle_config::Config;
use muelle_shell::{autostart, dock_link};
use tauri::State;
use tauri_plugin_updater::{Update, UpdaterExt};

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

/// The dock is installed next to this app.
fn dock_exe() -> io::Result<PathBuf> {
    Ok(env::current_exe()?.with_file_name("muelle.exe"))
}

#[tauri::command]
fn get_config() -> Config {
    muelle_config::load(&muelle_config::default_path())
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// The defaults live in `muelle-config`; the window asks for them to reset.
#[tauri::command]
fn default_config() -> Config {
    Config::default()
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
    let dock = dock_exe().map_err(|error| error.to_string())?;
    let target = enabled.then(|| dock.to_string_lossy().into_owned());
    autostart::set(target.as_deref()).map_err(|error| error.to_string())
}

/// The update found by the last check, kept until the user installs it.
#[derive(Default)]
struct PendingUpdate(Mutex<Option<Update>>);

/// Asks GitHub Releases for a newer version; returns it, if any. Only runs
/// while Settings is open: the dock itself never touches the network.
#[tauri::command]
async fn check_update(
    app: tauri::AppHandle,
    pending: State<'_, PendingUpdate>,
) -> Result<Option<String>, String> {
    let update = app
        .updater()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?;
    let version = update.as_ref().map(|update| update.version.clone());
    *pending.0.lock().map_err(|error| error.to_string())? = update;
    Ok(version)
}

/// Downloads the signed installer and runs it; the installer stops the dock,
/// replaces both apps and starts the dock again. This process exits.
#[tauri::command]
async fn install_update(pending: State<'_, PendingUpdate>) -> Result<(), String> {
    let update = pending
        .0
        .lock()
        .map_err(|error| error.to_string())?
        .take()
        .ok_or("no hay ninguna actualización pendiente")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())
}

fn main() {
    muelle_config::migrate_legacy_folder();
    // The Start menu entry opens this app: it also brings back a dock the user quit.
    if !dock_link::is_running()
        && let Err(error) = dock_exe().and_then(|dock| Command::new(dock).spawn())
    {
        eprintln!("muelle-settings: no se pudo abrir el dock: {error}");
    }
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(PendingUpdate::default())
        .invoke_handler(tauri::generate_handler![
            get_config,
            default_config,
            set_config,
            get_autostart,
            set_autostart,
            check_update,
            install_update
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        eprintln!("muelle-settings: {error}");
    }
}
