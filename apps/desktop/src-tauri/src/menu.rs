//! Native macOS menu bar, generated from `shortcuts.json` (the shortcut registry shared
//! with the UI). Menu accelerators win over the webview, which also lets app-level
//! shortcuts like ⌘↵ take precedence over Monaco's defaults. Selecting an item emits a
//! `menu` event carrying the command id; the UI dispatches it.

use serde::Deserialize;
use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::{App, Emitter};

const SHORTCUTS: &str = include_str!("../../shortcuts.json");

#[derive(Deserialize)]
struct Registry {
    commands: Vec<CommandDef>,
}

#[derive(Deserialize)]
struct CommandDef {
    id: String,
    label: String,
    menu: String,
    accelerator: Option<String>,
}

pub fn install(app: &mut App) -> tauri::Result<()> {
    let registry: Registry = serde_json::from_str(SHORTCUTS).expect("shortcuts.json is valid");

    let app_menu = SubmenuBuilder::new(app, "SnapTable")
        .about(None)
        .separator()
        .services()
        .separator()
        .hide()
        .hide_others()
        .show_all()
        .separator()
        .quit()
        .build()?;

    // Standard Edit items are required for copy/paste/undo to work inside the webview.
    let mut edit = SubmenuBuilder::new(app, "Edit")
        .undo()
        .redo()
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all();

    let mut submenus = Vec::new();
    for name in ["File", "View", "Query", "Window"] {
        let mut sub = SubmenuBuilder::new(app, name);
        for cmd in registry.commands.iter().filter(|c| c.menu == name) {
            let mut item = MenuItemBuilder::with_id(cmd.id.as_str(), cmd.label.as_str());
            if let Some(acc) = &cmd.accelerator {
                item = item.accelerator(acc);
            }
            sub = sub.item(&item.build(app)?);
        }
        if name == "Window" {
            sub = sub
                .separator()
                .minimize()
                .maximize()
                .separator()
                .fullscreen();
        }
        submenus.push(sub.build()?);
    }
    for cmd in registry.commands.iter().filter(|c| c.menu == "Edit") {
        let mut item = MenuItemBuilder::with_id(cmd.id.as_str(), cmd.label.as_str());
        if let Some(acc) = &cmd.accelerator {
            item = item.accelerator(acc);
        }
        edit = edit.item(&item.build(app)?);
    }
    let edit = edit.build()?;

    let mut menu = MenuBuilder::new(app).item(&app_menu);
    menu = menu.item(&submenus[0]).item(&edit);
    for sub in &submenus[1..] {
        menu = menu.item(sub);
    }
    app.set_menu(menu.build()?)?;

    app.on_menu_event(|app, event| {
        let _ = app.emit("menu", event.id().0.clone());
    });
    Ok(())
}
