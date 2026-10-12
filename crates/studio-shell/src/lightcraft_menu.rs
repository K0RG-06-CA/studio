//! LightCraft's menu bar, hosted by Studio in the persona bar row (D4): its own model
//! (`menubar::menu_bar`) drawn as its in-window menus are (`menubar::show_in_window`, whose item
//! drawing is private and adapted here), each item classified by [`route`] before it can run.
//! An item [`route`] doesn't know (a new upstream one) is drawn disabled and never runs, and
//! `every_lightcraft_menu_item_is_routed` fails until it is classified.

use lightcraft_ui_egui::LightcraftApp;
use lightcraft_ui_egui::menubar::{self, MenuNode};
use lightcraft_ui_egui::state::RightPanel;
use lightcraft_ui_egui::theme::Tokens;
use lightcraft_ui_egui::{i18n, icons};
use serde_json::{Value, json};
use studio_doc::Document;

/// What Studio does with a LightCraft menu item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// As LightCraft runs it (`run_item`): it saves, opens, closes, quits and undoes nothing of
    /// the document's (Save Metadata to File writes an XMP sidecar, never the photo; exports never
    /// write over an original).
    Pass,
    /// Studio's `edit.undo`/`edit.redo` while a document is open (its one history), LightCraft's
    /// own otherwise.
    History,
    /// Studio's `app.quit`: its close guard, which asks about unsaved changes.
    Quit,
    /// Changes the library under the open document: off while one is open.
    Library,
    /// Deletes photos: off while the document's photo is among them.
    Delete,
}

/// The LightCraft menu items that run as it runs them ([`Route::Pass`]), by menu.
const PASS: &[&str] = &[
    // File
    "file.addPhotos",
    "file.addFolder",
    "file.addFromDevice",
    "file.backupLibrary",
    "file.findMissing",
    "file.importPresets",
    "file.exportPresets",
    "library.import",
    "library.compact",
    "library.toggleAutoWriteXmp",
    "library.buildPreviews",
    "library.cancelPreviews",
    "library.smartPreviews",
    "library.clearPreviews",
    "dialog.newAlbum",
    "dialog.newFolder",
    "dialog.smartAlbum",
    "dialog.newSmartAlbum",
    "dialog.export",
    "app.export",
    "app.exportPrevious",
    // Edit
    "develop.copy",
    "dialog.copySettings",
    "develop.paste",
    "develop.pastePrevious",
    "dialog.pasteSettings",
    "develop.autoSync",
    "library.selectAll",
    "library.selectNone",
    "library.selectBy",
    "view.focusSearch",
    "app.settings",
    // View
    "view.photoGrid",
    "view.squareGrid",
    "view.gridInfo",
    "view.detail",
    "view.compare",
    "view.survey",
    "view.people",
    "view.reference",
    "view.leftPanel",
    "view.photoCounts",
    "view.faceBoxes",
    "view.filmstrip",
    "view.histogram",
    "view.navigator",
    "view.infoOverlay",
    "view.fullScreenPreview",
    "view.enterFullScreen",
    "view.secondWindow",
    "view.slideshow",
    "view.showOriginal",
    "view.beforeAfter",
    "view.beforeAfterSplit",
    "view.beforeAfterTopBottom",
    "view.beforeAfterSplitTopBottom",
    "beforeAfter.swap",
    "beforeAfter.copyAfterToBefore",
    "beforeAfter.copyBeforeToAfter",
    "beforeAfter.resetBefore",
    "view.zoomIn",
    "view.zoomOut",
    "view.zoomToggle",
    "view.zoomFit",
    "view.zoom100",
    "view.clipping",
    "view.softProof",
    "view.maskOverlay",
    "view.maskOverlayMode",
    "view.maskOverlayColor",
    "view.maskPins",
    "view.cropOverlay",
    "view.cropOverlayOrientation",
    "view.visualizeSpots",
    "library.sort",
    "library.shuffle",
    "view.filterBar",
    "library.clearFilter",
    "library.findSimilar",
    "compare.swap",
    "compare.makeSelect",
    // Photo
    "album.addPhotos",
    "album.create",
    "photo.rate",
    "photo.flag",
    "photo.pick",
    "photo.reject",
    "photo.unflag",
    "photo.label",
    "label.applySet",
    "dialog.labelNames",
    "metadata.applyPreset",
    "dialog.saveMetadataPreset",
    "view.autoAdvance",
    "photo.rotateLeft",
    "photo.rotateRight",
    "photo.flipHorizontal",
    "photo.flipVertical",
    "version.create",
    "photo.virtualCopy",
    "photo.duplicate",
    "photo.convertToDng",
    "photo.addToLibrary",
    "stack.group",
    "stack.ungroup",
    "stack.split",
    "stack.remove",
    "stack.setTop",
    "stack.moveUp",
    "stack.moveDown",
    "stack.toggle",
    "stack.expandAll",
    "stack.collapseAll",
    "dialog.autoStack",
    "dialog.mergeHdr",
    "dialog.mergePanorama",
    "dialog.mergeHdrPanorama",
    "merge.hdrLast",
    "merge.panoramaLast",
    "merge.hdrPanoramaLast",
    "develop.auto",
    "develop.treatment",
    "develop.reset",
    "develop.matchExposure",
    "dialog.createPreset",
    "preset.create",
    "photo.saveMetadataToFile",
    "photo.readMetadataFromFile",
    "photo.copyMetadata",
    "photo.pasteMetadata",
    "photo.reload",
    "app.showInFinder",
    "photo.editInExternal",
    "dialog.rename",
    "dialog.captureTime",
    "dialog.allMetadata",
    "dialog.cull",
    "photo.tagFromTracklog",
    "photo.restore",
    // Window
    "panel.edit",
    "panel.profiles",
    "panel.crop",
    "panel.remove",
    "panel.masking",
    "panel.redeye",
    "panel.presets",
    "panel.versions",
    "panel.activity",
    "panel.info",
    "panel.keywords",
    "section.light",
    "section.color",
    "section.effects",
    "section.detail",
    "section.optics",
    "tool.brush",
    "tool.linear",
    "tool.radial",
    "tool.wbPicker",
    "tool.guidedUpright",
    "brush.smaller",
    "brush.larger",
    "brush.featherLess",
    "brush.featherMore",
    // Help
    "app.help",
    "app.discord",
    "app.feedback",
    "app.website",
    "app.github",
    "app.artcraft",
    "app.whatsNew",
    "app.shortcuts",
    "app.systemInfo",
    "app.about",
];

/// LightCraft's Help links, relabelled: they go to its upstream project, not to Studio.
const UPSTREAM_LINKS: &[(&str, &str)] = &[
    ("app.help", "LightCraft Help (upstream)"),
    ("app.discord", "Join the ArtCraft Discord (LightCraft upstream)…"),
    ("app.feedback", "Send Feedback to LightCraft upstream…"),
    ("app.website", "LightCraft upstream website"),
    ("app.github", "LightCraft upstream on GitHub"),
    ("app.artcraft", "ArtCraft website (LightCraft upstream)"),
];

/// How Studio routes the LightCraft menu item `id`; none: not classified (a new upstream item),
/// so it is off.
pub fn route(id: &str) -> Option<Route> {
    use Route::*;
    Some(match id {
        "edit.undo" | "edit.redo" => History,
        "app.quit" => Quit,
        "app.openLibrary" | "file.restoreLibrary" => Library,
        "photo.delete" | "photo.deletePermanently" | "library.emptyRecentlyDeleted" => Delete,
        _ if lightcraft_ui_egui::menus::language_from_command(id).is_some() => Pass,
        _ if PASS.contains(&id) => Pass,
        _ => return None,
    })
}

/// Whether the menu item `id` may run now: classified, and not off for the open document.
pub fn allowed(id: &str, app: &LightcraftApp, doc: Option<&Document>) -> bool {
    let photo = doc.and_then(Document::photo);
    match route(id) {
        None => false,
        Some(Route::Library) => doc.is_none(),
        Some(Route::Delete) if id == "library.emptyRecentlyDeleted" => photo.is_none_or(|p| !app.session.catalog.photo(p).is_some_and(|p| p.deleted)),
        Some(Route::Delete) => photo.is_none_or(|p| !app.session.targets(&json!({})).contains(&p)),
        Some(Route::Pass | Route::History | Route::Quit) => true,
    }
}

/// LightCraft's Delete key deletes the selected photos as Photo › Delete does (outside its mask,
/// spot and red-eye tools, where it deletes what is being edited): true when that would take the
/// document's photo.
pub fn delete_key_takes_document_photo(app: &LightcraftApp, doc: Option<&Document>) -> bool {
    !matches!(app.ui.right, RightPanel::Masking | RightPanel::Remove | RightPanel::RedEye) && !allowed("photo.delete", app, doc)
}

/// A key as LightCraft's menu model writes it (`Cmd+Shift+S`).
pub fn shortcut_name(m: egui::Modifiers, k: egui::Key) -> String {
    let mods = [(m.command, "Cmd+"), (m.alt, "Alt+"), (m.shift, "Shift+")];
    mods.iter().filter(|(on, _)| *on).map(|(_, s)| *s).collect::<String>() + k.name()
}

/// LightCraft's menu bar as Studio shows it: an item is enabled only where [`allowed`], undo and
/// redo are Studio's while a document is open, Help's links say they are upstream's, and keys Studio takes (`taken`, as
/// [`shortcut_name`] writes them) show on no item.
pub fn bar(app: &LightcraftApp, doc: Option<&Document>, taken: &[String]) -> Vec<(String, Vec<MenuNode>)> {
    menubar::menu_bar(app).into_iter().map(|(title, nodes)| (title, adapt(nodes, app, doc, taken))).collect()
}

fn adapt(nodes: Vec<MenuNode>, app: &LightcraftApp, doc: Option<&Document>, taken: &[String]) -> Vec<MenuNode> {
    let item = |n| match n {
        MenuNode::Item { id, params, mut label, mut shortcut, mut enabled, checked } => {
            if let (Some(Route::History), Some(doc)) = (route(&id), doc) {
                let redo = id == "edit.redo";
                label = if redo { "Redo" } else { "Undo" }.to_owned();
                enabled = if redo { doc.history.can_redo() } else { doc.history.can_undo() };
            }
            if let Some((_, upstream)) = UPSTREAM_LINKS.iter().find(|(link, _)| *link == id) {
                label = (*upstream).to_owned();
            }
            enabled &= allowed(&id, app, doc);
            shortcut = shortcut.filter(|s| !taken.contains(s));
            MenuNode::Item { id, params, label, shortcut, enabled, checked }
        }
        MenuNode::Submenu { label, children } => MenuNode::Submenu { label, children: adapt(children, app, doc, taken) },
        MenuNode::Separator => MenuNode::Separator,
    };
    nodes.into_iter().map(item).collect()
}

/// Horizontal gap between menu titles (LightCraft's `TITLE_GAP`).
const TITLE_GAP: f32 = 24.0;

/// The menu titles, each with its dropdown, in LightCraft's tokens: the item clicked, if any.
pub fn show(ui: &mut egui::Ui, bar: &[(String, Vec<MenuNode>)]) -> Option<(String, Value)> {
    let t = Tokens::get(ui.ctx());
    let mac = ui.ctx().os() == egui::os::OperatingSystem::Mac;
    let mut clicked = None;
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.x = TITLE_GAP;
        for (title, items) in bar {
            let r = ui.add(egui::Button::new(egui::RichText::new(i18n::tr(title)).font(t.font(13.0)).color(t.text_label)).frame(false));
            egui::Popup::menu(&r).show(|ui| nodes_ui(ui, items, mac, &mut clicked));
        }
    });
    clicked
}

/// LightCraft's `menubar::nodes_ui`.
fn nodes_ui(ui: &mut egui::Ui, nodes: &[MenuNode], mac: bool, clicked: &mut Option<(String, Value)>) {
    ui.set_min_width(220.0);
    for n in nodes {
        match n {
            MenuNode::Separator => {
                ui.separator();
            }
            MenuNode::Submenu { label, children } => {
                ui.menu_button(format!("      {}", i18n::tr(label)), |ui| nodes_ui(ui, children, mac, clicked));
            }
            MenuNode::Item { id, params, label, shortcut, enabled, checked } => {
                // a gutter for check marks, like native menus
                let mut b = egui::Button::new(format!("      {}", menubar::display_item_label(id, params, label)));
                if let Some(sc) = shortcut {
                    b = b.shortcut_text(menubar::shortcut_text(sc, mac));
                }
                let r = ui.add_enabled(*enabled, b);
                if *checked == Some(true) {
                    let t = Tokens::get(ui.ctx());
                    let c = egui::Rect::from_center_size(egui::pos2(r.rect.left() + 11.0, r.rect.center().y), egui::vec2(13.0, 13.0));
                    icons::paint(ui.painter(), c, icons::Icon::Check, if *enabled { t.text } else { t.text_disabled });
                }
                if r.clicked() {
                    *clicked = Some((id.clone(), params.clone()));
                    ui.close();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every item id LightCraft's menus can show: its registry's menu entries (whichever the host
    /// shows) and what the bar expands into submenus.
    fn every_id(app: &LightcraftApp) -> std::collections::BTreeSet<String> {
        fn walk(nodes: &[MenuNode], ids: &mut std::collections::BTreeSet<String>) {
            for n in nodes {
                match n {
                    MenuNode::Item { id, .. } => {
                        ids.insert(id.clone());
                    }
                    MenuNode::Submenu { children, .. } => walk(children, ids),
                    MenuNode::Separator => {}
                }
            }
        }
        let mut ids: std::collections::BTreeSet<String> = lightcraft_ui_egui::menus::menu_entries(app).into_iter().map(|e| e.id).collect();
        for (_, nodes) in menubar::menu_bar(app) {
            walk(&nodes, &mut ids);
        }
        ids
    }

    /// Fail closed: a menu item upstream adds breaks this until it is classified.
    #[test]
    fn every_lightcraft_menu_item_is_routed() {
        let app = LightcraftApp::new(lightcraft_engine::Session::with_demo(), Default::default());
        let ids = every_id(&app);
        assert!(ids.len() > 100 && ids.contains("photo.saveMetadataToFile"), "{ids:?}");
        let unrouted: Vec<&String> = ids.iter().filter(|id| route(id).is_none()).collect();
        assert!(unrouted.is_empty(), "LightCraft menu items with no route: {unrouted:?}");
        // and what isn't classified never runs
        assert!(!allowed("app.somethingNew", &app, None));
    }

    #[test]
    fn shortcut_names_are_lightcrafts() {
        use egui::{Key, Modifiers};
        assert_eq!(shortcut_name(Modifiers::COMMAND, Key::S), "Cmd+S");
        assert_eq!(shortcut_name(Modifiers::COMMAND | Modifiers::SHIFT, Key::S), "Cmd+Shift+S");
        assert_eq!(shortcut_name(Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT, Key::E), "Cmd+Alt+Shift+E");
    }
}
