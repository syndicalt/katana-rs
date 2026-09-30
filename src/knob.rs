//! Custom rotary knob widget (modern, flat design).
//!
//! Drag vertically to change; double-click resets to a mid value; shift+drag
//! for fine steps. Renders an arc + indicator dot, value readout below.

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Stroke, Vec2};

/// How the knob maps its raw value to display.
pub struct KnobSpec {
    pub min: f64,
    pub max: f64,
    pub default: f64,
    /// optional display transform (e.g. percentage)
    pub fmt: Option<Box<dyn Fn(f64) -> String>>,
}

impl KnobSpec {
    pub fn percent() -> Self {
        KnobSpec { min: 0.0, max: 100.0, default: 50.0, fmt: Some(Box::new(|v| format!("{v:.0}%"))) }
    }
    pub fn normal() -> Self {
        KnobSpec { min: 0.0, max: 100.0, default: 50.0, fmt: None }
    }
    pub fn bipolar() -> Self {
        KnobSpec { min: -50.0, max: 50.0, default: 0.0, fmt: Some(Box::new(|v| format!("{v:+.0}"))) }
    }
    pub fn display(&self, v: f64) -> String {
        if let Some(f) = &self.fmt {
            f(v)
        } else if (self.max - self.min + 1.0) > 200.0 {
            format!("{v:.0}")
        } else {
            format!("{v}")
        }
    }
}

/// Draw a rotary knob. `value` is in display units matching `spec`.
pub fn knob(ui: &mut egui::Ui, id_salt: impl std::hash::Hash, value: &mut f64, spec: &KnobSpec) -> egui::Response {
    let desired = Vec2::splat(64.0);
    let (rect, mut resp) = ui.allocate_exact_size(desired, Sense::drag());
    if ui.is_rect_visible(rect) {
        let center = rect.center();
        let radius = rect.width() * 0.5 - 6.0;

        let painter = ui.painter_at(rect.expand(2.0));

        // background disc
        painter.circle_filled(center, radius, Color32::from_rgb(28, 28, 34));
        painter.circle_stroke(center, radius, Stroke::new(1.5, Color32::from_rgb(70, 70, 80)));

        let t = ((*value - spec.min) / (spec.max - spec.min)).clamp(0.0, 1.0) as f32;
        let angle_start = std::f32::consts::PI * 0.75;
        let angle = angle_start - t * (std::f32::consts::PI * 1.5);

        fn arc_points(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
            let n = 32;
            (0..=n).map(|i| {
                let a = a0 + (a1 - a0) * (i as f32 / n as f32);
                Pos2::new(c.x + r * a.cos(), c.y - r * a.sin())
            }).collect()
        }
        let arc_r = radius - 4.0;
        painter.add(egui::Shape::Path(egui::epaint::PathShape {
            points: arc_points(center, arc_r, angle_start, angle_start - std::f32::consts::PI * 1.5),
            closed: false,
            fill: Color32::TRANSPARENT,
            stroke: Stroke::new(3.0, Color32::from_rgb(80, 80, 92)).into(),
        }));
        if t > 0.001 {
            painter.add(egui::Shape::Path(egui::epaint::PathShape {
                points: arc_points(center, arc_r, angle_start, angle),
                closed: false,
                fill: Color32::TRANSPARENT,
                stroke: Stroke::new(3.0, Color32::from_rgb(0x5C, 0xA6, 0xE0)).into(),
            }));
        }

        let dot = Pos2::new(
            center.x + (radius - 10.0) * angle.cos(),
            center.y - (radius - 10.0) * angle.sin(),
        );
        painter.circle_filled(dot, 3.2, Color32::from_rgb(0xFF, 0xD1, 0x66));

        painter.text(
            center + Vec2::new(0.0, radius * 0.45),
            Align2::CENTER_CENTER,
            spec.display(*value),
            FontId::proportional(11.0),
            Color32::from_rgb(0xDD, 0xDD, 0xE8),
        );
    }

    let mut changed = false;
    if resp.dragged() {
        let dy = resp.drag_delta().y;
        let step = if ui.input(|i| i.modifiers.shift) { 0.5 } else { 2.0 };
        let new = *value + (-dy as f64) * step * (spec.max - spec.min) / 100.0;
        *value = new.clamp(spec.min, spec.max);
        changed = true;
    }
    if resp.double_clicked() {
        *value = spec.default;
        changed = true;
    }
    if changed {
        resp.mark_changed();
    }
    let _ = id_salt;
    resp
}
