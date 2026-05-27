mod commands;

use commands::{catalog, copy, drives, metadata, steam, vdf_isolation};
use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};

const TRAY_ID: &str = "main-tray";

#[tauri::command]
fn quit_app(app_handle: tauri::AppHandle) {
    app_handle.exit(0);
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(copy::CopyControlState::default())
        .invoke_handler(tauri::generate_handler![
            drives::scan_vault_ssd,
            drives::scan_local_steam_libraries,
            drives::discover_vault_letters,
            drives::scan_local_only_games,
            catalog::get_ssd_catalog,
            catalog::upsert_game,
            catalog::rescan_ssd,
            copy::copy_game,
            copy::push_to_vault,
            copy::copy_to_vault,
            copy::delete_from_vault,
            copy::hide_vault_manifest,
            copy::restore_vault_manifest,
            copy::discard_hidden_vault_manifest,
            vdf_isolation::isolate_vault_for_steam_update,
            vdf_isolation::restore_vault_from_isolation,
            copy::remove_local_game,
            copy::pause_copy,
            copy::resume_copy,
            copy::cancel_copy,
            steam::parse_library_folders_vdf,
            steam::register_game_in_steam,
            steam::launch_game,
            steam::check_installed_games,
            metadata::fetch_steam_metadata,
            metadata::resolve_app_id,
            metadata::check_vault_updates,
            metadata::read_local_appmanifest_state,
            quit_app,
        ])
        .setup(|app| {
            let show_item =
                MenuItem::with_id(app, "show", "Show Steam Vault Sync", true, None::<&str>)?;
            let separator = PredefinedMenuItem::separator(app)?;
            let exit_item =
                MenuItem::with_id(app, "exit", "Exit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_item, &separator, &exit_item])?;

            let icon = app
                .default_window_icon()
                .cloned()
                .ok_or("default window icon missing")?;

            TrayIconBuilder::with_id(TRAY_ID)
                .icon(icon)
                .tooltip("Steam Vault Sync")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "exit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| match event {
            // Hide to tray instead of exiting when the user clicks X.
            // Long-running copies continue running; tray tooltip reflects
            // their progress. Explicit exit comes from the tray menu or
            // the in-app Exit button.
            WindowEvent::CloseRequested { api, .. } => {
                let _ = window.hide();
                api.prevent_close();
            }
            // Minimize-to-tray: when Windows minimizes the window, hide it
            // entirely so the icon disappears from the taskbar and lives
            // only in the system tray.
            WindowEvent::Resized(_) => {
                if window.is_minimized().unwrap_or(false) {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running Steam Vault Sync");
}
