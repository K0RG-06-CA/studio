//! Panel docking (egui_dock): one dock tree per persona, with the canvas in the middle and panel
//! groups around it, laid out like the Adobe app the persona replaces. Every panel is an empty
//! placeholder until its milestone fills it.

use egui::{Id, Margin, Rect, Ui, WidgetText};
use egui_dock::{DockState, Node, NodeIndex, NodePath, Surface, TabStyle, TabViewer, Tree};
use serde::{Deserialize, Serialize};

use crate::Persona;
use crate::theme::{self, TOKENS};

/// A dock tab: the persona's canvas or a named panel.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tab {
    Canvas,
    Panel(String),
}

/// A column of stacked panel groups: (share of the column's height, tabs in the group).
type Column = &'static [(f32, &'static [&'static str])];

/// Default layout of a persona: side columns as (width share of the window, groups).
struct Spec {
    left: (f32, Column),
    right: (f32, Column),
}

fn spec(p: Persona) -> Spec {
    match p {
        // Lightroom Classic's places, with LightCraft's panels: its sidebar holds the catalog, folders
        // and collections; its right panel shows one thing at a time, so those panels are tabs of
        // one group. (No Navigator, Quick Develop or Histogram of their own in LightCraft.)
        Persona::Library => Spec { left: (0.18, &[(1.0, &["Folders"])]), right: (0.2, &[(1.0, &["Keywording", "Metadata"])]) },
        // History sits with Edit (both are LightCraft's right panel), not in the left column.
        Persona::Develop => Spec { left: (0.18, &[(1.0, &["Presets"])]), right: (0.2, &[(1.0, &["Edit", "History"])]) },
        Persona::Film => Spec {
            left: (0.18, &[(0.3, &["Navigator"]), (0.3, &["Film Stocks"]), (0.4, &["History"])]),
            right: (0.2, &[(0.2, &["Histogram"]), (0.8, &["Film Effects"])]),
        },
        // Photoshop: PhotoCraft's right dock (Layers, Properties) in one tab: its panels can't be
        // drawn one by one (see pixel). Above it, Studio's History: the document's one history,
        // both engines' steps. Tools and options are fixed chrome.
        Persona::Pixel => Spec { left: (0.0, &[]), right: (0.24, &[(0.3, &["History"]), (0.7, &["Layers"])]) },
        // Photoshop: tabbed groups in a right column (tools and options are fixed chrome).
        Persona::Vector | Persona::Layout | Persona::Export => Spec {
            left: (0.0, &[]),
            right: (
                0.22,
                &[(0.25, &["Histogram", "Navigator", "History"]), (0.45, &["Layers", "Channels", "Paths"]), (0.3, &["Properties", "Adjustments"])],
            ),
        },
    }
}

/// Collapsible sections inside a panel, like Lightroom's slider groups.
fn sections(panel: &str) -> &'static [&'static str] {
    match panel {
        "Edit" => &["Basic", "Tone Curve", "Color Mixer", "Color Grading", "Detail", "Lens Corrections", "Transform", "Effects", "Calibration"],
        // Fixed pipeline order (PRD › Film effects: full integration), halation at the top.
        "Film Effects" => &["Halation", "Lazy Blur", "Saturation RGB", "Lazy Bleach", "Grain", "Film Stock"],
        _ => &[],
    }
}

/// The persona's default dock tree.
pub fn default_layout(p: Persona) -> DockState<Tab> {
    let s = spec(p);
    let mut dock = DockState::new(vec![Tab::Canvas]);
    let tree = dock.main_surface_mut();
    let mut center = NodeIndex::root();
    // A split's fraction is the share of its left (or top) half.
    let mut rest = 1.0;
    if let Some((_, first)) = s.left.1.first() {
        let [c, left] = tree.split_left(center, s.left.0, tabs(first));
        center = c;
        rest -= s.left.0;
        stack(tree, left, s.left.1);
    }
    if let Some((_, first)) = s.right.1.first() {
        let [_, right] = tree.split_right(center, 1.0 - s.right.0 / rest, tabs(first));
        stack(tree, right, s.right.1);
    }
    dock
}

fn tabs(names: &[&str]) -> Vec<Tab> {
    names.iter().map(|n| Tab::Panel((*n).to_owned())).collect()
}

/// Split `node` (holding the first group) downwards so each group gets its share of the column.
fn stack(tree: &mut Tree<Tab>, mut node: NodeIndex, groups: Column) {
    let mut rest: f32 = groups.iter().map(|(w, _)| w).sum();
    for pair in groups.windows(2) {
        let [(w, _), (_, next)] = pair else { continue };
        // Clamped: egui_dock panics on a fraction outside 0..=1.
        let [_, below] = tree.split_below(node, (w / rest).clamp(0.05, 0.95), tabs(next));
        rest -= w;
        node = below;
    }
}

/// Whether a layout loaded from disk is safe to show. A corrupt or hand-edited settings file can
/// still parse while holding values egui panics on (a NaN split fraction crashes the first frame,
/// and the layout is reloaded at every launch), so check before showing it:
/// - every split fraction is a number in 0..=1;
/// - no stored rectangle is NaN (±inf is fine: it's how egui_dock marks "not laid out yet");
/// - the canvas is there exactly once.
pub fn layout_is_usable(dock: &DockState<Tab>) -> bool {
    let not_nan = |r: Rect| ![r.min.x, r.min.y, r.max.x, r.max.y].iter().any(|v| v.is_nan());
    let nodes_ok = dock.iter_surfaces().flat_map(|s| s.iter_nodes()).all(|n| match n {
        Node::Horizontal(s) | Node::Vertical(s) => (0.0..=1.0).contains(&s.fraction) && not_nan(s.rect),
        Node::Leaf(l) => not_nan(l.rect),
        Node::Empty => true,
    });
    let windows_ok = dock.iter_surfaces().all(|s| match s {
        Surface::Window(_, w) => not_nan(w.rect()),
        _ => true,
    });
    let canvases = dock.iter_all_tabs().filter(|(_, t)| **t == Tab::Canvas).count();
    nodes_ok && windows_ok && canvases == 1
}

/// A saved layout has exactly the panels the persona has now (a save from before a milestone
/// changed them isn't: Pixel's placeholders before PhotoCraft, Pixel without Studio's History).
/// Panels can't be closed, so a missing one means an older save.
pub fn tabs_known(p: Persona, dock: &DockState<Tab>) -> bool {
    let known = default_layout(p);
    dock.iter_all_tabs().all(|(_, t)| known.find_tab(t).is_some()) && known.iter_all_tabs().all(|(_, t)| dock.find_tab(t).is_some())
}

/// Draws the tabs of one persona's dock.
pub struct Viewer {
    pub persona: Persona,
    /// Set when the user picks Reset Workspace from a tab's right-click menu; the shell then runs
    /// `window.resetWorkspace` after the dock is drawn (the layout can't change while it is).
    pub reset_requested: bool,
}

impl TabViewer for Viewer {
    type Tab = Tab;

    fn id(&mut self, tab: &mut Tab) -> Id {
        match tab {
            Tab::Canvas => Id::new("studio.canvas"),
            Tab::Panel(name) => Id::new(("studio.panel", name.as_str())),
        }
    }

    fn title(&mut self, tab: &mut Tab) -> WidgetText {
        match tab {
            Tab::Canvas => self.canvas_title().into(),
            Tab::Panel(name) => name.as_str().into(),
        }
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Tab) {
        match tab {
            Tab::Canvas => {
                ui.centered_and_justified(|ui| {
                    ui.label(theme::placeholder(format!("{} canvas", self.persona.label())));
                });
            }
            Tab::Panel(name) => {
                let secs = sections(name);
                if secs.is_empty() {
                    ui.label(theme::placeholder("Empty"));
                }
                for s in secs {
                    egui::CollapsingHeader::new(*s).id_salt((name.as_str(), *s)).default_open(secs.first() == Some(s)).show(ui, |ui| {
                        ui.label(theme::placeholder("Empty"));
                    });
                }
            }
        }
    }

    // egui_dock paints tab titles without accessibility info; name them for screen readers.
    fn on_tab_button(&mut self, tab: &mut Tab, response: &egui::Response) {
        let title = self.title(tab).text().to_owned();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &title));
    }

    // Right-click a tab: the way back to the default layout until there is a Window menu.
    fn context_menu(&mut self, ui: &mut Ui, _tab: &mut Tab, _path: NodePath) {
        if ui.button("Reset Workspace").clicked() {
            self.reset_requested = true;
            ui.close();
        }
    }

    // Panels come back with Reset Workspace, so none can be closed for now.
    fn is_closeable(&self, _tab: &Tab) -> bool {
        false
    }

    fn allowed_in_windows(&self, tab: &mut Tab) -> bool {
        *tab != Tab::Canvas
    }

    fn scroll_bars(&self, tab: &Tab) -> [bool; 2] {
        if *tab == Tab::Canvas { [false, false] } else { [false, true] }
    }

    fn tab_style_override(&self, tab: &Tab, global: &TabStyle) -> Option<TabStyle> {
        let canvas = *tab == Tab::Canvas;
        canvas.then(|| {
            let mut s = global.clone();
            if canvas {
                s.tab_body.bg_fill = TOKENS.canvas;
            }
            s.tab_body.inner_margin = Margin::ZERO;
            s
        })
    }
}

impl Viewer {
    fn canvas_title(&self) -> &'static str {
        match self.persona {
            Persona::Library => "Grid",
            Persona::Develop | Persona::Film => "Loupe",
            Persona::Pixel | Persona::Vector | Persona::Layout | Persona::Export => "Untitled",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(d: &DockState<Tab>) -> Vec<String> {
        d.iter_all_tabs()
            .map(|(_, t)| match t {
                Tab::Canvas => "Canvas".to_owned(),
                Tab::Panel(n) => n.clone(),
            })
            .collect()
    }

    #[test]
    fn every_persona_has_a_canvas_and_its_adobe_panels() {
        for p in Persona::ALL {
            let n = names(&default_layout(p));
            assert_eq!(n.iter().filter(|t| *t == "Canvas").count(), 1, "{p:?}: {n:?}");
            if !p.lightroom() && p != Persona::Pixel {
                assert!(n.contains(&"Histogram".to_owned()), "{p:?}: {n:?}");
            }
        }
        assert!(names(&default_layout(Persona::Library)).contains(&"Folders".to_owned()));
        assert!(names(&default_layout(Persona::Develop)).contains(&"Edit".to_owned()));
        let mut pixel = names(&default_layout(Persona::Pixel));
        pixel.sort();
        assert_eq!(pixel, ["Canvas", "History", "Layers"]);
    }

    #[test]
    fn film_sections_follow_the_pipeline_order_with_halation_on_top() {
        assert_eq!(sections("Film Effects").first(), Some(&"Halation"));
        assert_eq!(sections("Film Effects").last(), Some(&"Film Stock"));
    }
}
