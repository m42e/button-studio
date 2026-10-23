use crate::model::DesignColor;
use eframe::egui::{Ui, color_picker::Alpha, color_picker::color_edit_button_hsva, ecolor::Hsva};

/// Edit a design color without losing its RGB channels when alpha is low or zero.
pub(crate) fn edit_design_color(ui: &mut Ui, color: &mut DesignColor) -> bool {
    let mut hsva = Hsva::from_srgb([color.red, color.green, color.blue]);
    hsva.a = color.alpha as f32 / 255.0;

    let response = color_edit_button_hsva(ui, &mut hsva, Alpha::OnlyBlend);
    if response.changed() {
        let [red, green, blue, alpha] = hsva.to_srgba_unmultiplied();
        *color = DesignColor {
            red,
            green,
            blue,
            alpha,
        };
    }
    response.changed()
}
