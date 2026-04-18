use std::sync::Arc;

use tauri::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use tracker_core::db::Project;

use crate::AppState;

pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let menu = build_menu(app)?;

    let _tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("Claude Tracker")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(handle_menu_event)
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { .. } = event {
                let app = tray.app_handle().clone();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;
    Ok(())
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::new(app)?;

    let open_dashboard = MenuItem::with_id(app, "open-dashboard", "Open Dashboard", true, None::<&str>)?;
    menu.append(&open_dashboard)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // "Recent" submenu — built on demand from the DB. We pre-populate with
    // the top 5 most recently-active projects if the DB is already open.
    if let Some(recent) = load_recent(app) {
        if !recent.is_empty() {
            let submenu = Submenu::new(app, "Recent", true)?;
            for p in recent {
                let id = format!("recent:{}", p.id);
                let label = format!("{} — {}", p.name, short_path(&p.path));
                let item = MenuItem::with_id(app, &id, &label, true, None::<&str>)?;
                submenu.append(&item)?;
            }
            menu.append(&submenu)?;
            menu.append(&PredefinedMenuItem::separator(app)?)?;
        }
    }

    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn load_recent(app: &AppHandle) -> Option<Vec<Project>> {
    let state = app.try_state::<Arc<AppState>>()?;
    let db = state.db.lock().ok()?;
    db.recent_active(5).ok()
}

fn handle_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id.as_ref() {
        "open-dashboard" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }
        "quit" => {
            app.exit(0);
        }
        other if other.starts_with("recent:") => {
            // Clicking a recent project reveals its folder on disk.
            let Some(id_str) = other.strip_prefix("recent:") else {
                return;
            };
            let Ok(id) = id_str.parse::<i64>() else { return };
            let Some(state) = app.try_state::<Arc<AppState>>() else {
                return;
            };
            let Ok(db) = state.db.lock() else { return };
            if let Ok(Some(p)) = db.get_project(id) {
                let _ = std::process::Command::new("open").arg(&p.path).spawn();
            }
        }
        _ => {}
    }
}

fn short_path(p: &str) -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    if !home.is_empty() && p.starts_with(&home) {
        return format!("~{}", &p[home.len()..]);
    }
    p.to_string()
}
