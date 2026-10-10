//! Studio's History panel (Pixel's History tab): the open document's one history, both engines'
//! steps in the order they were made, as Photoshop's History panel lists them. The current row
//! is highlighted, undone rows are dimmed below it, and each row says which persona made it; a
//! click jumps there (`history.goto`).

use egui::{RichText, Ui};
use studio_doc::history::Entry;

use crate::theme::TOKENS;

/// Draw the rows; the row clicked, if any.
pub fn show(ui: &mut Ui, rows: &[Entry], at: usize) -> Option<usize> {
    let t = TOKENS;
    let mut clicked = None;
    // the dock gives Pixel's tabs no margin (PhotoCraft's panels bring their own)
    let frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 4));
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        frame.show(ui, |ui| {
            for (i, row) in rows.iter().enumerate() {
                ui.horizontal(|ui| {
                    let label = RichText::new(&row.label).color(if i <= at { t.text } else { t.text_dim });
                    if ui.selectable_label(i == at, label).clicked() {
                        clicked = Some(i);
                    }
                    let made_in = match (row.develop, row.pixel) {
                        (true, true) => "Develop + Pixel",
                        (true, false) => "Develop",
                        (false, true) => "Pixel",
                        (false, false) => "",
                    };
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(RichText::new(made_in).color(t.text_dim)));
                });
            }
        });
    });
    clicked
}
