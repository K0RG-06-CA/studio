//! PhotoCraft (storytold) inside the Pixel persona, in Photoshop's places: its tool bar on the
//! left, its options bar on top, its canvas in Studio's canvas tab and its right dock in one dock
//! tab.
//!
//! PhotoCraft's Layers, History and Properties panels are private (`d836e34`), so they can't each
//! get a Studio dock tab: Studio shows PhotoCraft's whole right dock in one. The first example
//! compiles (and so its `compile_fail` doctest fails) once upstream makes the Layers panel public; then each
//! panel can get its own tab. The second one is the same call to a public panel, so the first
//! can't pass on a typo.
//!
//! ```compile_fail
//! fn layers(app: &mut photocraft_ui_egui::PhotocraftApp, ui: &mut egui::Ui) {
//!     photocraft_ui_egui::panels::layers(app, ui);
//! }
//! ```
//!
//! ```
//! fn toolbar(app: &mut photocraft_ui_egui::PhotocraftApp, ui: &mut egui::Ui) {
//!     photocraft_ui_egui::panels::toolbar(app, ui);
//! }
//! ```
