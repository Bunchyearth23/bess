//! Counts the interactive controls one frame puts on screen (W-008), by
//! accessibility role. The BeamNG/WAV export block is drawn inline in
//! `update` and is not included.
use super::*;
use egui::accesskit::Role;

fn count(scratch: Scratch) -> (usize, usize) {
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut app = App::with_ctx(&ctx, None);
    app.firing_text = firing_text(&scratch.design);
    app.scratch = Some(scratch);
    let mut roles = std::collections::BTreeMap::<String, usize>::new();
    let mut panel = |app: &mut App, f: fn(&mut App, &mut egui::Ui)| {
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| f(app, ui));
        });
        let update = output.platform_output.accesskit_update.expect("accesskit");
        for (_, node) in update.nodes {
            if matches!(
                node.role(),
                Role::Button
                    | Role::CheckBox
                    | Role::RadioButton
                    | Role::Slider
                    | Role::SpinButton
                    | Role::TextInput
                    | Role::ComboBox
                    | Role::Switch
            ) {
                *roles.entry(format!("{:?}", node.role())).or_default() += 1;
            }
        }
    };
    panel(&mut app, |a, ui| a.scratch_panel(ui));
    panel(&mut app, |a, ui| a.listen_controls(ui));
    panel(&mut app, |a, ui| a.driving_panel(ui));
    let sliders = roles.get("Slider").copied().unwrap_or(0);
    let spin = roles.get("SpinButton").copied().unwrap_or(0);
    let total: usize = roles.values().sum();
    eprintln!("roles {roles:?}");
    // A slider's value box is a SpinButton of its own: count it once.
    (total - sliders.min(spin), sliders)
}

#[test]
fn visible_controls_of_the_default_i4_and_a_v8_turbo() {
    let i4 = count(Scratch::default());
    let v8 = count(Scratch {
        design: bess::scratch::PRESETS[10].1,
        build: bess::engine_build::EngineBuild {
            aspiration: bess::engine_build::Aspiration::Turbo,
            ..Default::default()
        },
        ..Default::default()
    });
    eprintln!(
        "CONTROLS i4={} ({} sliders) v8t={} ({} sliders)",
        i4.0, i4.1, v8.0, v8.1
    );
    assert!(i4.0 > 0 && v8.0 >= i4.0);
}
