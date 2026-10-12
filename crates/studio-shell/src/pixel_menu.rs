//! PhotoCraft's menu bar, hosted by Studio in the persona bar row in Pixel (D4): its in-window
//! model (`menus::menu_items`, the layout its Windows build draws; its native-menu backend gets the
//! Mac one) drawn as its in-window bar draws it (`menus::menu_bar`, whose drawing is private and
//! adapted here), each item classified by [`route`] before it can run. Its in-window menus stay
//! hidden behind an empty native-menu backend (`pixel::hide_menus`).
//!
//! Fail closed: an item [`route`] doesn't know (a new upstream one) is drawn disabled and never
//! runs, and `every_photocraft_menu_item_is_routed` fails until it is classified. The items that
//! run as PhotoCraft runs them are listed in `pixel_menu_pass.txt`. The keys of PhotoCraft's items
//! go the same way ([`keys`]): a routed item's key runs Studio's command, an item that is off or
//! hidden has its key dropped.
//!
//! L19: PhotoCraft's command palette (Edit › Search…, its key and its title bar's search icon) runs
//! any command by id straight through its engine, past this routing (its File items, its own
//! undo). It has no host hook, so Edit › Search… is off and Studio closes the palette after every
//! frame (`Pixel::ui`): its icon opens it for that frame only, too soon for a command to run.

use std::cell::OnceCell;
use std::collections::HashSet;
use std::sync::LazyLock;

use egui::KeyboardShortcut;
use photocraft_ui_egui::menu_nav::{self, Nav};
use photocraft_ui_egui::menus::{self, MenuItem, TOP_MENUS};
use photocraft_ui_egui::{PhotocraftApp, i18n, shortcuts, theme::Tokens};
use studio_doc::Document;

/// What Studio does with a PhotoCraft menu item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// As PhotoCraft runs it (`menus::invoke`): it edits the open document (a Pixel step in
    /// Studio's history), or changes PhotoCraft's view, panels, tools or preferences. Its exports
    /// write new files named after the document or its layers where the user says (those of its
    /// own save dialog can't: Studio gives it no file services), never the document's file.
    Pass,
    /// Studio's `document.*` command: its prompt, its never-overwrite-the-source rule.
    Document(&'static str),
    /// Studio's `edit.undo`, `edit.redo` or `edit.toggleLastState`: the document's one history.
    History,
    /// Studio's History panel (`window.history`): PhotoCraft's own History shows PhotoCraft's
    /// states only (D1).
    HistoryPanel,
    /// Studio's `app.quit`: its close guard, which asks about unsaved changes.
    Quit,
    /// Drawn disabled and never run: it would make a second PhotoCraft document, which Studio's one
    /// document closes at once, write over the files it processes (the batch processors, given
    /// their input folder as output: the library's originals), or (Edit › Search…) open the
    /// palette (L19).
    Off,
    /// Left out of the menus: no meaning with Studio's one document (New, Open Recent, Close All,
    /// Close Others) or one Studio's history can't follow (Purge).
    Hidden,
}

/// The PhotoCraft menu items (and key bindings) that run as it runs them ([`Route::Pass`]).
static PASS: LazyLock<HashSet<&'static str>> =
    LazyLock::new(|| include_str!("pixel_menu_pass.txt").lines().filter(|l| !l.is_empty() && !l.starts_with('#')).collect());

/// PhotoCraft's Help links, relabelled: they go to its upstream project, not to Studio.
const UPSTREAM_LINKS: &[(&str, &str)] = &[
    ("help.discord", "Join the ArtCraft Discord (PhotoCraft upstream)\u{2026}"),
    ("help.website", "PhotoCraft upstream website"),
    ("help.artcraftWebsite", "ArtCraft website (PhotoCraft upstream)"),
    ("help.github", "PhotoCraft upstream on GitHub"),
    ("help.reportIssue", "Report an Issue to PhotoCraft upstream\u{2026}"),
];

/// How Studio routes the PhotoCraft menu item (or key binding) `id`; none: not classified (a new
/// upstream item), so it is off.
pub fn route(id: &str) -> Option<Route> {
    use Route::*;
    Some(match id {
        "file.open" | "file.openAs" => Document("document.open"),
        "file.save" => Document("document.save"),
        "file.saveAs" => Document("document.saveAs"),
        "file.saveACopy" => Document("document.saveACopy"),
        "file.revert" => Document("document.revert"),
        "file.close" => Document("document.close"),
        "file.exit" => Quit,
        "edit.undo" | "edit.redo" | "edit.toggleLastState" => History,
        "window.panel.history" => HistoryPanel,
        "file.new" | "file.newFromClipboard" | "file.closeAll" | "file.closeOthers" | "file.clearRecent" => Hidden,
        _ if id.starts_with("file.openRecent.") || id.starts_with("edit.purge.") => Hidden,
        "image.duplicate"
        | "layer.smartObjects.editContents"
        | "file.automate.cropAndStraightenPhotos"
        | "file.automate.photomerge"
        | "file.automate.mergeToHdrPro"
        | "file.automate.contactSheetII"
        | "file.scripts.loadFilesIntoStack"
        | "file.scripts.statistics"
        // batch processors: given their input folder as output, they write over the originals
        | "file.automate.batch"
        | "file.scripts.imageProcessor"
        | "file.automate.lensCorrection"
        | "edit.search" => Off,
        _ if PASS.contains(id) => Pass,
        _ => return None,
    })
}

/// Whether the menu item `id` may run now: classified, not off, and with what it needs of the
/// open document (Revert: a file to go back to, and changes since).
pub fn allowed(id: &str, doc: Option<&Document>) -> bool {
    match route(id) {
        None | Some(Route::Off | Route::Hidden) => false,
        Some(Route::Document("document.open") | Route::Pass | Route::Quit | Route::HistoryPanel) => true,
        Some(Route::Document("document.revert")) => doc.is_some_and(|d| d.path.is_some() && d.unsaved()),
        Some(Route::Document(_)) => doc.is_some(),
        Some(Route::History) => doc.is_some_and(|d| match id {
            "edit.undo" => d.history.can_undo(),
            "edit.redo" => d.history.can_redo(),
            _ => d.history.can_undo() || d.history.can_redo(),
        }),
    }
}

/// PhotoCraft's menu items as Studio shows them: hidden items left out, an item enabled only where
/// [`allowed`] (a routed one by Studio's state, not PhotoCraft's), Window › History checked by
/// Studio's panel (`history_shown`), Help's links saying they are upstream's, and keys Studio takes
/// (`taken`) shown on no item.
pub fn items(app: &PhotocraftApp, doc: Option<&Document>, history_shown: bool, taken: &[KeyboardShortcut]) -> Vec<MenuItem> {
    let mut items = menus::menu_items(app);
    items.retain(|it| it.label == "---" || route(&it.id) != Some(Route::Hidden));
    // and a submenu left with only separators (Purge) with them
    let real: HashSet<Vec<String>> =
        items.iter().filter(|it| it.label != "---").flat_map(|it| (1..=it.path.len()).map(|n| it.path[..n].to_vec())).collect();
    items.retain(|it| it.label != "---" || real.contains(&it.path));
    for it in items.iter_mut().filter(|it| it.label != "---") {
        let own = matches!(route(&it.id), Some(Route::Pass)).then_some(it.enabled);
        it.enabled = own.unwrap_or(true) && allowed(&it.id, doc);
        if route(&it.id) == Some(Route::HistoryPanel) {
            it.checked = Some(history_shown);
        }
        if let Some((_, upstream)) = UPSTREAM_LINKS.iter().find(|(link, _)| *link == it.id) {
            it.label = (*upstream).to_owned();
        }
        if it.shortcut.as_deref().and_then(shortcuts::parse).is_some_and(|sc| taken.contains(&sc)) {
            it.shortcut = None;
        }
    }
    items
}

/// PhotoCraft's keys that Studio takes in Pixel, by what their item routes to: Studio's command
/// to run, or none (dropped: the item is off, hidden or unclassified). Undo, redo and quit keys
/// aren't here (Studio's own keys, `UNDO_KEYS`, take them only while it has a document and the
/// user isn't typing), nor are the keys of items that run as PhotoCraft runs them.
pub fn keys(app: &PhotocraftApp) -> Vec<(KeyboardShortcut, Option<&'static str>)> {
    photocraft_ui_egui::shortcut_dispatch::bindings(app)
        .into_iter()
        .filter_map(|(id, sc)| match route(&id) {
            Some(Route::Pass | Route::History | Route::Quit) => None,
            Some(Route::Document(cmd)) => Some((sc, Some(cmd))),
            Some(Route::HistoryPanel) => Some((sc, Some("window.history"))),
            Some(Route::Off | Route::Hidden) | None => Some((sc, None)),
        })
        .collect()
}

/// Take the presses of PhotoCraft's `keys` (as PhotoCraft matches them): their commands, in the
/// order pressed (a dropped key has none).
pub fn take_keys(i: &mut egui::InputState, keys: &[(KeyboardShortcut, Option<&'static str>)]) -> Vec<&'static str> {
    let mut ids = Vec::new();
    i.events.retain(|e| {
        let hit = match e {
            egui::Event::Key { key, pressed: true, modifiers, .. } => keys.iter().find(|(sc, _)| shortcuts::key_matches(sc, *key, *modifiers)),
            _ => None,
        };
        ids.extend(hit.and_then(|(_, cmd)| *cmd));
        hit.is_none()
    });
    ids
}

/// PhotoCraft's menu titles and their dropdowns, as its in-window bar draws them, from `items`
/// (built only while a menu is open, as PhotoCraft's are: building walks every item's state): the
/// item clicked, if any (with Alt: its alternative, as in PhotoCraft's menus).
pub fn show(ui: &mut egui::Ui, items: impl FnOnce() -> Vec<MenuItem>) -> Option<String> {
    let mut build = Some(items);
    let items: OnceCell<Vec<MenuItem>> = OnceCell::new();
    let lang = i18n::current();
    let mut clicked: Option<String> = None;
    let t = Tokens::get(ui.ctx());
    let mut nav = Nav::load(ui.ctx());
    // egui's menu bar takes the whole width it is given (PhotoCraft's fills its title bar): here,
    // only its titles' (each with its 6 pt padding), so the persona buttons follow them
    let font = egui::TextStyle::Button.resolve(ui.style());
    let width: f32 =
        TOP_MENUS.iter().map(|top| ui.painter().layout_no_wrap(i18n::tr(lang, top).to_owned(), font.clone(), t.text_dim).size().x + 12.0).sum();
    ui.allocate_ui(egui::vec2(width.ceil() + 1.0, ui.available_height()), |ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let v = &mut ui.style_mut().visuals;
        v.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        v.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        v.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, t.text_dim);
        v.widgets.hovered.bg_stroke = egui::Stroke::NONE;
        egui::MenuBar::new().ui(ui, |ui| {
            ui.spacing_mut().button_padding = egui::vec2(6.0, 3.0);
            nav.bar_bottom = Some(ui.max_rect().bottom());
            let mut buttons = Vec::with_capacity(TOP_MENUS.len());
            for top in TOP_MENUS {
                let title = ui.add(egui::Button::new(egui::RichText::new(i18n::tr(lang, top)).color(t.text_dim)));
                let bar = egui::containers::menu::MenuConfig::find(ui);
                let config = egui::containers::menu::MenuConfig::new().close_behavior(bar.close_behavior).style(bar.style.clone());
                let open = title_press(ui.ctx(), &title);
                let opening = title.clicked() && ui.ctx().data(|d| d.get_temp::<bool>(press_gesture_id())).unwrap_or(false);
                let close = if opening {
                    egui::PopupCloseBehavior::IgnoreClicks
                } else if top == "Help" {
                    egui::PopupCloseBehavior::CloseOnClickOutside
                } else {
                    config.close_behavior
                };
                egui::Popup::menu(&title)
                    .open_memory(open)
                    .close_behavior(close)
                    .style(config.style.clone())
                    .info(egui::UiStackInfo::new(egui::UiKind::Menu).with_tag_value(egui::containers::menu::MenuConfig::MENU_CONFIG_TAG, config))
                    .show(|ui| {
                        let items = items.get_or_init(|| build.take().map_or_else(Vec::new, |f| f()));
                        let mine: Vec<&MenuItem> = items.iter().filter(|i| i.path.first().map(String::as_str) == Some(top)).collect();
                        ui.set_min_width(220.0);
                        if top == "Help" {
                            menu_nav::level(ui, 1, &mut nav, |ui, nav| {
                                help_search(ui, items, &mut clicked, nav);
                                render_level_rows(ui, &mine, 1, &mut clicked, nav);
                            });
                        } else {
                            render_level(ui, &mine, 1, &mut clicked, &mut nav);
                        }
                    });
                buttons.push(title);
            }
            switch_on_hover(ui.ctx(), &buttons);
            let tops: Vec<egui::Id> = buttons.iter().map(egui::Popup::default_response_id).collect();
            nav.keys(ui.ctx(), &tops, &mut clicked);
        });
    });
    nav.store(ui.ctx());
    if ui.input(|i| i.pointer.primary_released() || !i.pointer.primary_down()) {
        ui.ctx().data_mut(|d| d.remove::<bool>(press_gesture_id()));
    }
    clicked.map(|id| menus::alt_click(id, ui.input(|i| i.modifiers.alt)))
}

// What follows is PhotoCraft's `menus` (title_press, released_on, switch_on_hover, help_search,
// render_level_rows, …), private there; unchanged but for the labels (`label`).

fn press_gesture_id() -> egui::Id {
    egui::Id::new("menu-press-gesture")
}

fn title_press(ctx: &egui::Context, title: &egui::Response) -> Option<egui::SetOpenCommand> {
    let pressed = ctx.input(|i| i.pointer.primary_pressed()) && (title.is_pointer_button_down_on() || title.clicked());
    if !pressed {
        return None;
    }
    let open = egui::Popup::is_id_open(ctx, egui::Popup::default_response_id(title));
    if !open {
        ctx.data_mut(|d| d.insert_temp(press_gesture_id(), true));
    }
    Some(egui::SetOpenCommand::Bool(!open))
}

fn released_on(ui: &egui::Ui, item: &egui::Response) -> bool {
    item.enabled()
        && item.contains_pointer()
        && ui.input(|i| i.pointer.primary_released())
        && ui.ctx().data(|d| d.get_temp::<bool>(press_gesture_id())).unwrap_or(false)
}

fn pointer_reaches_title(ctx: &egui::Context, response: &egui::Response, p: egui::Pos2) -> bool {
    response.interact_rect.contains(p) && ctx.layer_id_at(p) == Some(response.layer_id)
}

fn switch_on_hover(ctx: &egui::Context, buttons: &[egui::Response]) {
    let ids: Vec<egui::Id> = buttons.iter().map(egui::Popup::default_response_id).collect();
    let Some(open) = ids.iter().position(|id| egui::Popup::is_id_open(ctx, *id)) else { return };
    let Some(p) = ctx.pointer_hover_pos() else { return };
    if ctx.input(|i| i.pointer.delta() == egui::Vec2::ZERO) {
        return;
    }
    if let Some(i) = buttons.iter().position(|b| pointer_reaches_title(ctx, b, p))
        && i != open
        && let Some(id) = ids.get(i)
    {
        egui::Popup::open_id(ctx, *id);
        ctx.request_repaint();
    }
}

/// An item's label in the UI language; Help's relabelled links stay as Studio wrote them.
fn label(lang: i18n::Lang, item: &MenuItem) -> String {
    if UPSTREAM_LINKS.iter().any(|(id, _)| *id == item.id) {
        return item.label.clone();
    }
    if item.id == "file.export.quickExportAsPng"
        && let Some(fmt) = item.label.strip_prefix("Quick Export as ")
    {
        return i18n::tr_id(lang, &item.id, "Quick Export as PNG").replace("PNG", fmt);
    }
    i18n::tr_id(lang, &item.id, &item.label).to_string()
}

fn help_search(ui: &mut egui::Ui, items: &[MenuItem], clicked: &mut Option<String>, nav: &mut Nav) {
    let ctx = ui.ctx().clone();
    let lang = i18n::current();
    let text_id = egui::Id::new("help-menu-search");
    let pass_id = text_id.with("pass");
    let pass = ctx.cumulative_pass_nr();
    let reopened = ctx.data(|d| d.get_temp::<u64>(pass_id)).is_none_or(|p| p + 1 < pass);
    ctx.data_mut(|d| d.insert_temp(pass_id, pass));
    let mut query: String = if reopened { String::new() } else { ctx.data(|d| d.get_temp(text_id)).unwrap_or_default() };
    let field =
        ui.add(egui::TextEdit::singleline(&mut query).id(text_id.with("field")).hint_text(i18n::tr(lang, "Search menus")).desired_width(220.0));
    if reopened {
        field.request_focus();
    }
    let results = menus::search_items(items, &query, lang);
    if field.lost_focus()
        && ui.input(|i| i.key_pressed(egui::Key::Enter))
        && let Some(it) = results.iter().find(|it| it.enabled)
    {
        *clicked = Some(it.id.clone());
        ui.close();
    }
    ctx.data_mut(|d| d.insert_temp(text_id, query.clone()));
    if !query.trim().is_empty() {
        if results.is_empty() {
            ui.weak(i18n::tr(lang, "No matching commands"));
        }
        for it in results {
            let mut trail: Vec<String> = it.path.iter().map(|p| i18n::tr(lang, p).to_string()).collect();
            trail.push(label(lang, it));
            let mut b = egui::Button::new(trail.join(" \u{203a} "));
            if let Some(sc) = &it.shortcut {
                b = b.shortcut_text(shortcuts::pretty(sc));
            }
            let hit = nav.row(ui, 0, it.enabled, Some(&it.id), |ui, _| {
                let r = ui.add_enabled(it.enabled, b);
                let hit = r.clicked() || released_on(ui, &r);
                (r, hit)
            });
            if hit {
                *clicked = Some(it.id.clone());
                ui.close();
            }
        }
    }
    ui.separator();
}

fn render_level(ui: &mut egui::Ui, items: &[&MenuItem], depth: usize, clicked: &mut Option<String>, nav: &mut Nav) {
    menu_nav::level(ui, depth, nav, |ui, nav| render_level_rows(ui, items, depth, clicked, nav));
}

const MENU_SEPARATOR: f32 = 9.0;

fn menu_tint(name: &str) -> Option<egui::Color32> {
    Some(match name {
        "red" => egui::Color32::from_rgb(190, 60, 60),
        "orange" => egui::Color32::from_rgb(200, 120, 40),
        "yellow" => egui::Color32::from_rgb(190, 170, 40),
        "green" => egui::Color32::from_rgb(60, 150, 70),
        "blue" => egui::Color32::from_rgb(50, 110, 200),
        "violet" => egui::Color32::from_rgb(130, 80, 190),
        "gray" => egui::Color32::from_rgb(120, 120, 120),
        _ => return None,
    })
}

fn render_level_rows(ui: &mut egui::Ui, items: &[&MenuItem], depth: usize, clicked: &mut Option<String>, nav: &mut Nav) {
    let t = Tokens::get(ui.ctx());
    let lang = i18n::current();
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    ui.spacing_mut().item_spacing.y = 0.0;
    if t.pro {
        let v = &mut ui.style_mut().visuals;
        v.widgets.hovered.weak_bg_fill = t.accent;
        v.widgets.hovered.bg_fill = t.accent;
        v.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);
        v.widgets.hovered.corner_radius = egui::CornerRadius::same(3);
        ui.spacing_mut().button_padding = egui::vec2(10.0, 4.0);
    }
    let mut shown_subs: Vec<&str> = Vec::new();
    let mut last_was_sep = true;
    for (i, it) in items.iter().enumerate() {
        if it.path.len() == depth {
            if it.label == "---" {
                if !last_was_sep && i + 1 < items.len() {
                    ui.add(egui::Separator::default().spacing(MENU_SEPARATOR));
                    last_was_sep = true;
                }
                continue;
            }
            let mut text = label(lang, it);
            if let Some(c) = it.checked {
                text = format!("{} {}", if c { "\u{2714}" } else { "  " }, text);
            }
            let mut b = egui::Button::new(text);
            if let Some(tint) = it.color.as_deref().and_then(menu_tint) {
                b = b.fill(tint.gamma_multiply(0.55));
            }
            if let Some(sc) = &it.shortcut {
                b = b.shortcut_text(shortcuts::pretty(sc));
            }
            let hit = nav.row(ui, depth - 1, it.enabled, Some(&it.id), |ui, _| {
                let r = ui.add_enabled(it.enabled, b);
                let r = match it.id.as_str() {
                    "image.mode.bits8" | "image.mode.bits16" => r.on_hover_text(i18n::tr(lang, "Integer")),
                    "image.mode.bits32" => r.on_hover_text(i18n::tr(lang, "Floating point")),
                    _ => r,
                };
                let hit = r.clicked() || released_on(ui, &r);
                (r, hit)
            });
            if hit {
                *clicked = Some(it.id.clone());
                ui.close();
            }
            last_was_sep = false;
        } else if let Some(name) = it.path.get(depth).map(String::as_str) {
            if shown_subs.contains(&name) {
                continue;
            }
            shown_subs.push(name);
            let child: Vec<&MenuItem> = items.iter().copied().filter(|c| c.path.get(depth).map(String::as_str) == Some(name)).collect();
            let any_enabled = child.iter().any(|c| c.enabled && c.label != "---");
            let enabled = any_enabled || !child.is_empty();
            ui.add_enabled_ui(enabled, |ui| {
                nav.row(ui, depth - 1, enabled, None, |ui, nav| {
                    let r = ui.menu_button(i18n::tr(lang, name), |ui| render_level(ui, &child, depth + 1, clicked, nav));
                    if r.inner.is_some() {
                        nav.set_anchor(depth, r.response.rect);
                    }
                    (r.response, ())
                });
            });
            last_was_sep = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PhotoCraft with a document, as Studio gives it one.
    fn app() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.session.add_document(crate::tests_photocraft::pixel_doc(), None);
        app.sync_views();
        app
    }

    /// Every id PhotoCraft's menus and keys can run: the menu items (with no document and with
    /// one: Open Recent grows), and every key binding.
    fn every_id() -> std::collections::BTreeSet<String> {
        let mut ids = std::collections::BTreeSet::new();
        let empty = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        for app in [empty, app()] {
            ids.extend(menus::menu_items(&app).into_iter().filter(|it| it.label != "---").map(|it| it.id));
            ids.extend(photocraft_ui_egui::shortcut_dispatch::bindings(&app).into_iter().map(|(id, _)| id));
        }
        ids
    }

    /// Fail closed: a menu item or key binding upstream adds breaks this until it is classified.
    #[test]
    fn every_photocraft_menu_item_is_routed() {
        let ids = every_id();
        assert!(ids.len() > 600 && ids.contains("image.adjustments.levels") && ids.contains("tools.swapColors"), "{}", ids.len());
        let unrouted: Vec<&String> = ids.iter().filter(|id| route(id).is_none()).collect();
        assert!(unrouted.is_empty(), "PhotoCraft menu items with no route: {unrouted:?}");
        // the pass list names only what PhotoCraft has (a renamed id would linger there unseen)
        let stale: Vec<&&str> = PASS.iter().filter(|id| !ids.contains(**id)).collect();
        assert!(stale.is_empty(), "pass-listed ids PhotoCraft no longer has: {stale:?}");
        // and what isn't classified never runs
        assert!(!allowed("file.somethingNew", None));
        assert!(items(&app(), None, true, &[]).iter().filter(|it| it.label != "---").all(|it| route(&it.id).is_some()));
    }

    /// Review (HIGH): PhotoCraft's batch processors write their results into the folder they're
    /// given, named after their inputs: given the input folder (Batch's and Lens Correction's
    /// format is "same"), they rewrite the library's originals in place, the document's photo
    /// among them, past Studio's never-overwrite rule. They are off, and shown disabled.
    #[test]
    fn batch_processors_that_can_write_over_their_inputs_are_off() {
        let shown = items(&app(), None, true, &[]);
        for id in ["file.automate.batch", "file.scripts.imageProcessor", "file.automate.lensCorrection"] {
            assert_eq!(route(id), Some(Route::Off), "{id}");
            assert_eq!(shown.iter().find(|it| it.id == id).map(|it| it.enabled), Some(false), "{id}");
        }
    }

    /// What would make a second PhotoCraft document is off: Studio's one document would close it at
    /// once, its result silently gone (review: Statistics stacks its files into a new document).
    #[test]
    fn items_that_make_a_second_document_are_off() {
        for id in ["image.duplicate", "file.scripts.loadFilesIntoStack", "file.scripts.statistics", "file.automate.photomerge"] {
            assert_eq!(route(id), Some(Route::Off), "{id}");
        }
    }

    /// PhotoCraft's keys go where their items route: Studio's command, or dropped.
    #[test]
    fn photocrafts_keys_follow_their_items() {
        let keys = keys(&app());
        let of = |s: &str| shortcuts::parse(s).and_then(|sc| keys.iter().find(|(k, _)| *k == sc)).map(|(_, cmd)| *cmd);
        assert_eq!(of("F12"), Some(Some("document.revert")));
        assert_eq!(of("Cmd+Alt+Shift+O"), Some(Some("document.open")));
        assert_eq!(of("Cmd+Alt+S"), Some(Some("document.saveACopy")));
        assert_eq!(of("Cmd+Alt+W"), Some(None), "Close All: dropped");
        assert_eq!(of("Cmd+Alt+P"), Some(None), "Close Others: dropped");
        assert_eq!(of("Cmd+K"), Some(None), "the palette: dropped");
        assert_eq!(of("Cmd+L"), None, "Levels: PhotoCraft's own");
        assert_eq!(of("Cmd+Z"), None, "undo: Studio's own keys take it, or the text field");
    }

    /// The items Studio hides are absent from the model it draws; those it routes are there.
    #[test]
    fn hidden_items_are_absent_from_studios_menus() {
        let shown: Vec<String> = items(&app(), None, true, &[]).into_iter().map(|it| it.id).collect();
        for id in
            ["file.new", "file.newFromClipboard", "file.closeAll", "file.closeOthers", "file.clearRecent", "edit.purge.all", "edit.purge.histories"]
        {
            assert!(!shown.iter().any(|s| s == id), "{id} is shown");
        }
        for id in ["file.open", "file.openAs", "file.save", "file.saveAs", "file.saveACopy", "file.revert", "file.close", "file.exit", "edit.undo"] {
            assert!(shown.iter().any(|s| s == id), "{id} is missing");
        }
    }
}
