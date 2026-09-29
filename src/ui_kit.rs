//! Widgets and pure helpers for the engine builder's tuning overrides
//! (W-007.3) and the label conventions shared by every slider (W-008).
use super::wheel_adjust;
use bess::engine_build::{EngineBuild, EngineTuning};
use eframe::egui::{self, Color32, RichText};
use std::ops::RangeInclusive;

/// Slider ranges of the `EngineTuning` overrides: the same numbers as
/// `EngineTuning::validate` (a test keeps them in step).
pub mod range {
    use std::ops::RangeInclusive;
    pub const DURATION_DEG: RangeInclusive<f32> = 180.0..=300.0;
    pub const LIFT_MM: RangeInclusive<f32> = 7.0..=16.0;
    pub const LSA_DEG: RangeInclusive<f32> = 102.0..=120.0;
    pub const ADVANCE_DEG: RangeInclusive<f32> = -4.0..=10.0;
    pub const INTAKE_TO_BORE: RangeInclusive<f32> = 0.25..=0.55;
    pub const EXHAUST_TO_BORE: RangeInclusive<f32> = 0.17..=0.53;
    pub const ROD_TO_STROKE: RangeInclusive<f32> = 1.4..=2.2;
    pub const THROTTLE_MM: RangeInclusive<f32> = 30.0..=110.0;
    pub const PLENUM_RATIO: RangeInclusive<f32> = 0.3..=3.0;
    pub const TURBO_SIZE: RangeInclusive<f32> = 0.5..=2.0;
}

/// What each override is worth while it is `None`, in the units of its slider.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Derived {
    pub duration_deg: f32,
    pub lift_mm: f32,
    pub lsa_deg: f32,
    pub advance_deg: f32,
    pub intake_to_bore: f32,
    pub exhaust_to_bore: f32,
    pub rod_to_stroke: f32,
    pub throttle_mm: f32,
    pub plenum_ratio: f32,
    pub turbo_size: f32,
}

impl Derived {
    /// From `resolve()` with no override, so the estimates keep one home.
    pub fn of(build: &EngineBuild, cylinders: u32) -> Self {
        let r = EngineTuning::default().resolve(build, cylinders);
        let bore = f64::from(build.bore_mm) * 1e-3;
        let stroke = f64::from(build.stroke_mm) * 1e-3;
        let displacement = f64::from(build.cylinder_litres()) * cylinders as f64 * 1e-3;
        Self {
            duration_deg: r.duration_at_050_deg as f32,
            lift_mm: (r.lift_m * 1e3) as f32,
            lsa_deg: (360. - r.exhaust_center_deg) as f32,
            advance_deg: (720. - r.exhaust_center_deg - r.intake_center_deg) as f32,
            intake_to_bore: (r.intake_diameter_m / bore) as f32,
            exhaust_to_bore: (r.exhaust_diameter_m / bore) as f32,
            rod_to_stroke: (r.rod_m / stroke) as f32,
            throttle_mm: (2e3 * (r.throttle_area_m2 / std::f64::consts::PI).sqrt()) as f32,
            plenum_ratio: (r.plenum_volume_m3 / displacement) as f32,
            turbo_size: 1.,
        }
    }
}

/// Edit a value that is derived while `slot` is `None`: the closure sees the
/// override or the derived value; a change makes it an override.
pub fn edit_override(
    slot: &mut Option<f32>,
    derived: f32,
    edit: impl FnOnce(&mut f32) -> bool,
) -> bool {
    let mut value = slot.unwrap_or(derived);
    let changed = edit(&mut value);
    if changed {
        *slot = Some(value);
    }
    changed
}

/// Slider for one override. Derived: shown with the derived value and a
/// "derived" tag. Overridden: a "Reset" button returns it to derived.
pub fn override_slider(
    ui: &mut egui::Ui,
    label: &str,
    slot: &mut Option<f32>,
    derived: f32,
    range: RangeInclusive<f32>,
    unit: &str,
    help: &str,
) {
    let (min, max) = (*range.start(), *range.end());
    let decimals = if max - min >= 20. {
        0
    } else if max - min >= 5. {
        1
    } else {
        2
    };
    ui.horizontal_wrapped(|ui| {
        edit_override(slot, derived, |value| {
            let mut response = ui.add(
                egui::Slider::new(value, range)
                    .suffix(unit)
                    .max_decimals(decimals)
                    .text(label),
            );
            wheel_adjust(ui, &mut response, value, min, max);
            response
                .on_hover_text(format!(
                    "{help}\nDerived from the parts: {derived:.decimals$}{unit}. Move it to override.",
                ))
                .changed()
        });
        if slot.is_some() {
            if ui
                .small_button("Reset")
                .on_hover_text(format!("Back to the derived {derived:.decimals$}{unit}"))
                .clicked()
            {
                *slot = None;
            }
        } else {
            ui.weak("derived");
        }
    });
}

/// Button that returns a whole group to its default; off when already there.
pub fn reset_button<T: Default + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    group: &mut T,
    tip: &str,
) {
    if ui
        .add_enabled(*group != T::default(), egui::Button::new(label).small())
        .on_hover_text(tip)
        .clicked()
    {
        *group = T::default();
    }
}

/// Tag for controls that shape the sound only: the torque curve does not
/// change (wave action does not feed back on the valves, X-021).
pub fn sound_only_tag(ui: &mut egui::Ui) {
    ui.label(
        RichText::new("sound only")
            .small()
            .italics()
            .color(Color32::from_rgb(140, 170, 200)),
    )
    .on_hover_text("Changes what you hear, not the torque curve: pressure waves do not feed back on the valves in this model.");
}

/// Valve overlap at 0.050″ in degrees, for the same duration on both cams.
pub fn overlap_deg(duration: f32, lsa: f32, advance: f32) -> f32 {
    duration - (2. * lsa - advance)
}

/// `"Bass (dB)"` becomes `("Bass", " dB")`, so units read as slider suffixes
/// and not as part of every label.
pub fn split_unit(label: &str) -> (&str, &'static str) {
    const UNITS: [(&str, &str); 14] = [
        ("dB", " dB"),
        ("Hz", " Hz"),
        ("rpm", " rpm"),
        ("mm", " mm"),
        ("m", " m"),
        ("ms", " ms"),
        ("kg·m²", " kg·m²"),
        ("Nm", " Nm"),
        ("kg", " kg"),
        ("degrees", "°"),
        ("seconds", " s"),
        ("%", " %"),
        ("×", " ×"),
        ("× part", " × stock"),
    ];
    if let Some(open) = label.rfind(" (")
        && let Some(unit) = label[open + 2..].strip_suffix(')')
        && let Some((_, suffix)) = UNITS.iter().find(|(name, _)| *name == unit)
    {
        return (&label[..open], suffix);
    }
    (label, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_shows_derived_some_edits_and_reset_returns_to_none() {
        let mut slot = None;
        // Looking without changing keeps it derived and shows the derived value.
        let mut seen = 0.;
        assert!(!edit_override(&mut slot, 230., |v| {
            seen = *v;
            false
        }));
        assert_eq!((slot, seen), (None, 230.));
        // An edit starts from the derived value and fixes the override.
        assert!(edit_override(&mut slot, 230., |v| {
            *v += 10.;
            true
        }));
        assert_eq!(slot, Some(240.));
        // Once fixed, the override is what the widget shows.
        edit_override(&mut slot, 230., |v| {
            seen = *v;
            false
        });
        assert_eq!(seen, 240.);
        // A group reset returns every field to derived.
        let mut tuning = EngineTuning::default();
        tuning.cam.duration_deg = slot;
        tuning.cam.lsa_deg = Some(108.);
        tuning.turbo.size = Some(1.2);
        tuning.cam = Default::default();
        assert!(tuning.cam == Default::default() && tuning.turbo.size == Some(1.2));
    }

    #[test]
    fn derived_values_match_what_resolve_uses() {
        let build = EngineBuild::default();
        let d = Derived::of(&build, 4);
        // cam 0.3: duration 218, lift 10.2, LSA 111.6, advance 0.
        assert!((d.duration_deg - 218.).abs() < 1e-3, "{}", d.duration_deg);
        assert!((d.lift_mm - 10.2).abs() < 1e-3);
        assert!((d.lsa_deg - 111.6).abs() < 1e-3);
        assert!(d.advance_deg.abs() < 1e-3);
        assert!((d.intake_to_bore - 0.36).abs() < 1e-6 && (d.exhaust_to_bore - 0.31).abs() < 1e-6);
        assert_eq!((d.rod_to_stroke, d.turbo_size), (1.75, 1.));
        // Feeding every derived value back as an override changes nothing.
        let mut tuning = EngineTuning::default();
        tuning.cam.duration_deg = Some(d.duration_deg);
        tuning.cam.lift_mm = Some(d.lift_mm);
        tuning.cam.lsa_deg = Some(d.lsa_deg);
        tuning.valves.intake_to_bore = Some(d.intake_to_bore);
        tuning.bottom.rod_to_stroke = Some(d.rod_to_stroke);
        tuning.intake.throttle_mm = Some(d.throttle_mm);
        tuning.intake.plenum_ratio = Some(d.plenum_ratio);
        let (a, b) = (
            tuning.resolve(&build, 4),
            EngineTuning::default().resolve(&build, 4),
        );
        assert!((a.throttle_area_m2 / b.throttle_area_m2 - 1.).abs() < 1e-4);
        assert!((a.plenum_volume_m3 / b.plenum_volume_m3 - 1.).abs() < 1e-4);
        assert!((a.lift_m / b.lift_m - 1.).abs() < 1e-4);
        assert!((overlap_deg(218., 111.6, 0.) - (218. - 223.2)).abs() < 1e-3);
    }

    #[test]
    fn slider_ranges_are_the_validated_ranges() {
        use range::*;
        type Set = fn(&mut EngineTuning, f32);
        let fields: [(&RangeInclusive<f32>, Set); 10] = [
            (&DURATION_DEG, |t, v| t.cam.duration_deg = Some(v)),
            (&LIFT_MM, |t, v| t.cam.lift_mm = Some(v)),
            (&LSA_DEG, |t, v| t.cam.lsa_deg = Some(v)),
            (&ADVANCE_DEG, |t, v| t.cam.intake_advance_deg = Some(v)),
            (&INTAKE_TO_BORE, |t, v| t.valves.intake_to_bore = Some(v)),
            (&EXHAUST_TO_BORE, |t, v| t.valves.exhaust_to_bore = Some(v)),
            (&ROD_TO_STROKE, |t, v| t.bottom.rod_to_stroke = Some(v)),
            (&THROTTLE_MM, |t, v| t.intake.throttle_mm = Some(v)),
            (&PLENUM_RATIO, |t, v| t.intake.plenum_ratio = Some(v)),
            (&TURBO_SIZE, |t, v| t.turbo.size = Some(v)),
        ];
        for (range, set) in fields {
            let (lo, hi) = (*range.start(), *range.end());
            let mut t = EngineTuning::default();
            for inside in [lo, hi] {
                set(&mut t, inside);
                assert!(t.validate().is_ok(), "{inside} rejected");
            }
            for outside in [lo - 0.01, hi + 0.01] {
                set(&mut t, outside);
                assert!(t.validate().is_err(), "{outside} accepted");
            }
        }
    }

    #[test]
    fn units_move_from_the_label_to_the_suffix() {
        assert_eq!(split_unit("Bass (dB)"), ("Bass", " dB"));
        assert_eq!(
            split_unit("Rotating inertia (kg·m²)"),
            ("Rotating inertia", " kg·m²")
        );
        assert_eq!(
            split_unit("Header length (× part)"),
            ("Header length", " × stock")
        );
        assert_eq!(split_unit("Cycle variation (×)"), ("Cycle variation", " ×"));
        assert_eq!(
            split_unit("Ignition retard (degrees)"),
            ("Ignition retard", "°")
        );
        // Not a unit: left alone.
        assert_eq!(split_unit("Body focus (Q)"), ("Body focus (Q)", ""));
        assert_eq!(split_unit("Bank angle"), ("Bank angle", ""));
    }
}
