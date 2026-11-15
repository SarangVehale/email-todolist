use engine::core::Core;
use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_notification::NotificationExt;

/// Single entry point for the UI: every command goes through `Core::rpc`.
#[tauri::command(async)]
fn rpc(core: tauri::State<Arc<Core>>, cmd: String, args: Option<serde_json::Value>) -> Result<serde_json::Value, String> {
    core.rpc(&cmd, &args.unwrap_or_default()).map_err(|e| e.to_string())
}

fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main(app)))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--minimized"])))
        .invoke_handler(tauri::generate_handler![rpc])
        .setup(|app| {
            // Local, not roaming: mail text must not sync to a domain profile server on managed Windows PCs.
            let dir = app.path().app_local_data_dir()?;
            engine::diag::install_crash_log(dir.clone());
            let handle = app.handle().clone();
            let notify: engine::core::Notify = Arc::new(move |title, body| {
                let _ = handle.notification().builder().title(title).body(body).show();
            });
            let core = Core::new(dir, notify);
            core.start();
            app.manage(core);

            let open = MenuItem::with_id(app, "open", "Open Inbox Agent", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().expect("window icon"))
                .tooltip("Inbox Agent")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let tauri::tray::TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, .. } = e {
                        show_main(tray.app_handle());
                    }
                })
                .build(app)?;

            if std::env::args().any(|a| a == "--minimized") {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.hide();
                }
            }
            Ok(())
        })
        // closing the window keeps the watcher running in the tray; the first time, a notification says so (unless desktop notifications are off)
        .on_window_event(|w, e| {
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
                let app = w.app_handle();
                let desktop_alerts = app.try_state::<Arc<Core>>().is_some_and(|c| c.settings().alerts.desktop);
                if desktop_alerts && app.path().app_local_data_dir().is_ok_and(|dir| engine::tray::told_once(&dir)) {
                    let icon = if cfg!(target_os = "macos") { "Its icon in the menu bar" } else { "Its icon near the clock" };
                    let _ = app
                        .notification()
                        .builder()
                        .title("Inbox Agent is still running")
                        .body(format!("It keeps watching your mail. {icon} opens it again."))
                        .show();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Inbox Agent");
}
