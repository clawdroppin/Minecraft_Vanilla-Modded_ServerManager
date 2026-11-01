mod commands;
mod core;
mod error;
mod models;
mod paths;
mod state;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager};

fn show_main(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // A second launch just focuses the running manager - two managers would fight over servers.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let loc = paths::resolve(app.handle())?;
            // The main window is created here (not from config) so a portable install can keep the
            // WebView2 profile inside its own folder instead of %LOCALAPPDATA%.
            let cfg = app.config().app.windows.iter().find(|w| w.label == "main").cloned().expect("main window config");
            let make = |data_dir: Option<std::path::PathBuf>| -> tauri::Result<tauri::WebviewWindow> {
                let mut builder = tauri::WebviewWindowBuilder::from_config(app.handle(), &cfg)?;
                if let Some(d) = data_dir {
                    builder = builder.data_directory(d);
                }
                // Debug builds only: expose the WebView over CDP for automated UI testing.
                #[cfg(debug_assertions)]
                if let Ok(port) = std::env::var("MCSM_CDP_PORT") {
                    builder = builder.additional_browser_args(&format!(
                        "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --remote-debugging-port={port}"
                    ));
                }
                builder.build()
            };
            let profile = loc.portable.then(|| loc.dir.join("webview"));
            // A WebView2 left over from a crashed/killed run keeps the profile locked: the next
            // window either hangs blank or fails with "resource is in use". Clear orphans first
            // (cheap process scan; WMI only when one is suspected), then fall back to a separate
            // recovery profile for this session – it only holds UI cache.
            if core::proc::orphan_webview_suspected() && core::proc::kill_orphaned_webviews() > 0 {
                std::thread::sleep(std::time::Duration::from_millis(600));
            }
            if let Err(e) = make(profile.clone()) {
                eprintln!("Window creation failed ({e}); retrying with a recovery WebView profile");
                core::proc::kill_orphaned_webviews();
                let base = match &profile {
                    Some(p) => p.clone(),
                    None => app.path().app_local_data_dir()?,
                };
                make(Some(base.join(format!("webview-recovery-{}", std::process::id()))))?;
            } else {
                // Normal start: drop recovery profiles from earlier sessions (best effort).
                let base = match &profile {
                    Some(p) => Some(p.clone()),
                    None => app.path().app_local_data_dir().ok(),
                };
                if let Some(Ok(rd)) = base.map(std::fs::read_dir) {
                    for e in rd.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("webview-recovery-")) {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
            app.manage(Arc::new(state::AppState::load(loc.dir, loc.portable)));

            let title = MenuItem::with_id(app, "title", "Minecraft Server Manager", false, None::<&str>)?;
            let show = MenuItem::with_id(app, "show", "Open", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit = MenuItem::with_id(app, "quit", "Quit (servers keep running)", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&title, &sep, &show, &quit])?;
            TrayIconBuilder::with_id("main-tray")
                .icon(app.default_window_icon().cloned().expect("icon"))
                .tooltip("Minecraft Server Manager")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id().as_ref() {
                    "show" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;
            // Started by Windows at login with --minimized: stay in the tray, automation still runs.
            if !std::env::args().any(|a| a == "--minimized") {
                show_main(app.handle());
            }
            core::supervisor::spawn(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Window close (X, Alt+F4, taskbar): the UI decides - tray or quit.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "main" {
                    api.prevent_close();
                    let _ = window.emit("app://close-requested", ());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_info,
            commands::get_settings,
            commands::save_settings,
            commands::cancel_task,
            commands::test_webhook,
            commands::app_quit,
            commands::window_hide,
            commands::probe_folder,
            commands::game_versions,
            commands::loader_versions,
            commands::version_meta,
            commands::list_instances,
            commands::get_statuses,
            commands::suggest_ports,
            commands::create_instance,
            commands::update_instance,
            commands::reorder_instances,
            commands::delete_instance,
            commands::clone_instance,
            commands::import_existing,
            commands::install_server,
            commands::change_version,
            commands::check_update,
            commands::start_server,
            commands::stop_server,
            commands::restart_server,
            commands::start_countdown,
            commands::cancel_countdown,
            commands::send_command,
            commands::get_launch_line,
            commands::preview_launch_line,
            commands::clear_crash_state,
            commands::list_java,
            commands::probe_java,
            commands::install_java,
            commands::list_config_files,
            commands::read_config,
            commands::write_config,
            commands::list_config_history,
            commands::read_text_file,
            commands::get_server_icon,
            commands::set_server_icon,
            commands::read_log_tail,
            commands::list_log_files,
            commands::list_worlds,
            commands::activate_world,
            commands::import_world,
            commands::export_world,
            commands::delete_world,
            commands::reset_world,
            commands::rename_world,
            commands::list_datapacks,
            commands::add_datapacks,
            commands::remove_datapack,
            commands::player_lists,
            commands::player_action,
            commands::kick_player,
            commands::player_stats,
            commands::setup_bedrock,
            commands::list_dir,
            commands::ai_models,
            commands::ai_chat,
            commands::ai_stop,
            commands::lookup_player,
            commands::list_content,
            commands::add_content_files,
            commands::set_content_enabled,
            commands::remove_content,
            commands::identify_content,
            commands::content_updates,
            commands::search_content,
            commands::project_versions,
            commands::install_content,
            commands::update_content,
            commands::export_client_pack,
            commands::install_modpack,
            commands::create_backup,
            commands::list_backups,
            commands::get_backup_dir,
            commands::restore_backup,
            commands::delete_backup,
            commands::export_pack,
            commands::read_pack,
            commands::import_pack,
            commands::firewall_state,
            commands::add_firewall_rules,
            commands::remove_firewall_rules,
            commands::net_info,
            commands::upnp_probe,
            commands::upnp_map,
            commands::upnp_unmap,
            commands::ping_server,
            commands::external_check,
            commands::run_diagnostics,
            commands::disk_usage,
            commands::read_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Minecraft Server Manager");
}
