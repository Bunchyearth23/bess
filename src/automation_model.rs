//! Provenance-checked Automation components translated to the physical engine.
//! This reconstructs a model, not the original recordings or an exact dyno map.
use crate::{
    bank::Bank,
    drive::Controls,
    engine_build::{Aspiration, BlockMaterial, Catalyst, Crankshaft, Fuel, Head, Headers, Muffler},
    engine_definition::EngineDefinition,
    engine_meta::{EngineMeta, Layout as SourceLayout},
    hybrid::Settings,
    project::Parameters,
    scratch::{EngineDesign, Layout, PRESETS, Scratch, SoundTuning},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct AutomationModel {
    pub scratch: Scratch,
    /// Neutral reconstruction of the verified source, before user edits.
    pub baseline: EngineDefinition,
    pub provenance: Vec<ParameterProvenance>,
    pub assumptions: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueOrigin {
    Read,
    Converted,
    Estimated,
    Modified,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ParameterProvenance {
    pub field: String,
    pub origin: ValueOrigin,
    pub baseline_origin: ValueOrigin,
    pub source: Option<String>,
    pub baseline: Value,
    pub value: Value,
    pub detail: String,
}

fn values(definition: &EngineDefinition) -> Result<BTreeMap<String, Value>, String> {
    fn flatten(path: &str, value: Value, fields: &mut BTreeMap<String, Value>) {
        if let Value::Object(object) = value {
            for (name, value) in object {
                let child = if path.is_empty() {
                    name
                } else {
                    format!("{path}.{name}")
                };
                flatten(&child, value, fields);
            }
        } else {
            // Arrays are a single editable value: cylinder order/banks/pins
            // must be interpreted together, not as independent measurements.
            fields.insert(path.into(), value);
        }
    }
    let mut fields = BTreeMap::new();
    let mut value = serde_json::to_value(definition).map_err(|e| e.to_string())?;
    // Project files omit untouched tuning, but provenance needs a stable field
    // universe so a first override (and its reset) is always represented.
    value["tuning"] = serde_json::to_value(definition.tuning).map_err(|e| e.to_string())?;
    flatten("", value, &mut fields);
    Ok(fields)
}

fn provenance(
    meta: &EngineMeta,
    definition: &EngineDefinition,
    assumptions: &[String],
) -> Result<Vec<ParameterProvenance>, String> {
    use ValueOrigin::{Converted, Estimated, Read};
    let p = &meta.physical;
    let mut imported: BTreeMap<&str, (ValueOrigin, String, String)> = BTreeMap::new();
    let mut record = |field, origin, source: String, detail: &str| {
        imported.insert(field, (origin, source, detail.into()));
    };
    record(
        "design.cylinders",
        Read,
        format!("Family.BlockConfig: {} cylinders", meta.cylinders),
        "Verified engine architecture",
    );
    record(
        "design.layout",
        Converted,
        format!("Family.BlockType: {:?}", meta.layout),
        "Source architecture mapped to the physical layout",
    );
    record(
        "design.bank_angle",
        Converted,
        format!("Family.BlockType: {:?}", meta.layout),
        "Bank angle decoded from the source architecture",
    );
    if let (Some(bore), Some(stroke)) = (p.bore_m, p.stroke_m) {
        record(
            "build.bore_mm",
            Converted,
            format!("Variant.Bore: {bore} m"),
            "Metres converted to millimetres; capacity checked",
        );
        record(
            "build.stroke_mm",
            Converted,
            format!("Variant.Stroke: {stroke} m"),
            "Metres converted to millimetres; capacity checked",
        );
    }
    for (field, source, input, output) in [
        (
            "build.compression",
            "Variant.Compression",
            p.compression,
            definition.build.compression,
        ),
        (
            "build.cam",
            "Variant.CamProfileSetting",
            p.cam_profile,
            definition.build.cam,
        ),
        (
            "idle_rpm",
            ".pc $idleRPM / engine JBeam idleRPM",
            p.idle_rpm,
            definition.idle_rpm,
        ),
        (
            "redline_rpm",
            "Variant.RPMLimit",
            p.rpm_limit,
            definition.redline_rpm,
        ),
    ] {
        if let Some(value) = input.filter(|v| v.is_finite()) {
            let exact = value == output;
            record(
                field,
                if exact { Read } else { Converted },
                format!("{source}: {value}"),
                if exact {
                    "Verified source value"
                } else {
                    "Source value limited to the supported physical range"
                },
            );
        }
    }
    if let Some(valves) = p.valves {
        record(
            "build.valves",
            Read,
            format!("Family.Valves: {valves}"),
            "Valves per cylinder decoded from the source tag",
        );
    }
    for (field, name, key, tag) in [
        (
            "build.block",
            "Block material",
            "Family.BlockMaterial",
            &p.block_material,
        ),
        ("build.head", "Cylinder head", "Family.Head", &p.head),
        ("build.crank", "Crank material", "Variant.Crank", &p.crank),
        (
            "build.fuel",
            "Fuel system",
            "Variant.FuelSystem",
            &p.fuel_system,
        ),
        ("build.vvt", "VVT", "Variant.VVT", &p.vvt),
        ("build.headers", "Headers", "Variant.Headers", &p.headers),
        ("build.catalyst", "Catalyst", "Variant.Cat", &p.catalyst),
    ] {
        if let Some(tag) = tag {
            let fallback = assumptions
                .iter()
                .any(|note| note.starts_with(&format!("{name}: absent or unsupported")));
            record(
                field,
                if fallback { Estimated } else { Converted },
                format!("{key}: {tag}"),
                if fallback {
                    "Unsupported source category; physical default used"
                } else {
                    "Source category mapped to its physical equivalent"
                },
            );
        }
    }
    let turbo = meta.turbocharged == Some(true);
    let known_layout = !turbo
        || matches!(
            p.aspiration_setup.as_deref(),
            Some("Turbo_Single_Name" | "Turbo_Twin_Name")
        );
    record(
        "build.aspiration",
        if known_layout { Converted } else { Estimated },
        format!(
            "Variant.AspirationType turbo={turbo}; AspirationSetup={:?}",
            p.aspiration_setup
        ),
        if known_layout {
            "Verified aspiration mapped to physical induction"
        } else {
            "Turbo confirmed; single-turbo layout estimated"
        },
    );
    if turbo && let Some(boost) = p.boost_setting {
        record(
            "build.boost_bar",
            Estimated,
            format!("Variant.ChargerMaxBoost_1: {boost}"),
            "Source scalar interpreted as nominal gauge bar; not a measured boost map",
        );
    }
    record(
        "build.exhaust_mm",
        Estimated,
        format!("Variant.ExhaustDiameter: {:?}", p.exhaust_diameter),
        "Raw source unit remains unverified; diameter estimated from capacity",
    );
    record(
        "build.muffler",
        Estimated,
        format!(
            "Variant.Muffler1={:?}; Muffler2={:?}",
            p.muffler_1, p.muffler_2
        ),
        "Two silencers reduced to one restrictive class; dimensions and packing estimated",
    );
    values(definition).map(|fields| fields.into_iter().map(|(field, value)| {
        let (origin, source, detail) = imported.remove(field.as_str()).map(|(origin, source, detail)| (origin, Some(source), detail)).unwrap_or((Estimated, None, "Physical-model estimate or neutral control default; not an exported source measurement".into()));
        ParameterProvenance { field, origin, baseline_origin: origin, source, baseline: value.clone(), value, detail }
    }).collect())
}

fn bounded(
    value: Option<f32>,
    fallback: f32,
    lo: f32,
    hi: f32,
    name: &str,
    notes: &mut Vec<String>,
) -> f32 {
    let value = match value.filter(|v| v.is_finite()) {
        Some(v) => v,
        None => {
            notes.push(format!("{name}: source absent; estimated {fallback}."));
            fallback
        }
    };
    let supported = value.clamp(lo, hi);
    if supported != value {
        notes.push(format!(
            "{name}: source {value} limited to supported {supported}."
        ));
    }
    supported
}

fn component<T: Copy>(value: Option<T>, fallback: T, name: &str, notes: &mut Vec<String>) -> T {
    value.unwrap_or_else(|| {
        notes.push(format!(
            "{name}: absent or unsupported source tag; physical default estimated."
        ));
        fallback
    })
}

impl AutomationModel {
    /// The public project/settings resolver: both the active edits and the
    /// original imported reference survive future changes to the mapper.
    pub fn from_settings(bank: &Bank, settings: &Settings) -> Result<Self, String> {
        let mut model = Self::resolve(
            bank,
            settings
                .engine
                .as_ref()
                .or(settings.engine_baseline.as_ref()),
            &settings.physical_sound,
        )?;
        if settings.engine.is_none() && settings.engine_baseline.is_some() {
            let mut definition = EngineDefinition::from_scratch(&model.scratch);
            definition.sound = settings.physical_sound;
            model = model.with_definition(&definition)?;
        }
        if let Some(baseline) = &settings.engine_baseline {
            model = model.with_baseline(baseline)?;
        }
        Ok(model)
    }

    /// Keep the saved reference independent of the current importer. Older
    /// projects did not save per-field provenance, so changed historical
    /// estimates must never be relabelled as current measured source values.
    pub fn with_baseline(&self, baseline: &EngineDefinition) -> Result<Self, String> {
        baseline.validate()?;
        let saved = values(baseline)?;
        let mut model = self.clone();
        model.baseline = *baseline;
        for item in &mut model.provenance {
            if let Some(value) = saved.get(&item.field) {
                if item.baseline != *value {
                    item.baseline_origin = ValueOrigin::Estimated;
                    item.detail = "Saved import reference differs from the current mapper; preserved historical model value".into();
                }
                item.baseline = value.clone();
                item.origin = if item.value == item.baseline {
                    item.baseline_origin
                } else {
                    ValueOrigin::Modified
                };
            }
        }
        Ok(model)
    }

    /// Resolve every rendering path from one definition. Source verification
    /// still runs when an override exists; edits never replace source identity.
    pub fn resolve(
        bank: &Bank,
        definition: Option<&EngineDefinition>,
        fallback_sound: &SoundTuning,
    ) -> Result<Self, String> {
        let model = Self::from_bank(bank)?;
        let mut effective = definition.copied().unwrap_or(model.baseline);
        if definition.is_none() {
            effective.sound = *fallback_sound;
        }
        model.with_definition(&effective)
    }

    /// Refresh active values and provenance after an edit without inspecting
    /// the archive or rebuilding the imported reference.
    pub fn with_definition(&self, definition: &EngineDefinition) -> Result<Self, String> {
        definition.validate()?;
        let active = values(definition)?;
        let mut model = self.clone();
        model.scratch = definition.to_scratch();
        for item in &mut model.provenance {
            if let Some(value) = active.get(&item.field) {
                item.value = value.clone();
                item.origin = if item.value == item.baseline {
                    item.baseline_origin
                } else {
                    ValueOrigin::Modified
                };
            }
        }
        Ok(model)
    }

    /// Offline construction only. The bank has already checked blend UID,
    /// Family/Variant linkage, active .pc part and engine JBeam identity.
    pub fn from_bank(bank: &Bank) -> Result<Self, String> {
        if !bank.min_rpm.is_finite()
            || !bank.max_rpm.is_finite()
            || bank.min_rpm < 0.
            || bank.max_rpm > 12000.
            || bank.min_rpm > bank.max_rpm
        {
            return Err("Physical Automation resynthesis requires an ordered source RPM range within 0–12000 RPM".into());
        }
        let meta = bank
            .engine_meta
            .as_ref()
            .ok_or("Physical resynthesis requires verified Automation engine metadata")?;
        let filename = bank.source.blend.rsplit('/').next().unwrap_or("");
        if !filename
            .strip_suffix(".sfxBlend2D.json")
            .is_some_and(|uid| uid.eq_ignore_ascii_case(&meta.uid))
        {
            return Err("Automation metadata does not match the selected audio source UID".into());
        }
        Self::from_meta(meta, bank.min_rpm, bank.max_rpm)
    }

    fn from_meta(meta: &EngineMeta, bank_min: f32, bank_max: f32) -> Result<Self, String> {
        let displacement = meta
            .displacement_l
            .filter(|v| v.is_finite() && *v > 0.)
            .ok_or("Verified Automation capacity is required for physical resynthesis")?;
        let turbo = meta
            .turbocharged
            .ok_or("Automation aspiration type is unknown")?;
        if !(1..=12).contains(&meta.cylinders) {
            return Err("Unsupported Automation cylinder count".into());
        }
        let p = &meta.physical;
        let mut notes = Vec::new();
        let (layout, bank_angle) = match meta.layout {
            SourceLayout::Inline => (Layout::Inline, 0.),
            SourceLayout::V { bank_angle_degrees } => {
                if !(1..180).contains(&bank_angle_degrees) || !meta.cylinders.is_multiple_of(2) {
                    return Err("Unsupported Automation V architecture".into());
                }
                (Layout::V, f32::from(bank_angle_degrees))
            }
        };
        let mut design = if layout == Layout::V && meta.cylinders == 8 && bank_angle == 90. {
            notes.push("Firing order, crankpins and cam revolutions: estimated typical cross-plane V8; not exported source metadata.".into());
            PRESETS.iter().find(|p| p.0 == "V8 cross-plane").unwrap().1
        } else {
            notes.push("Firing order, crankpins and cam revolutions: estimated evenly spaced sequential firing; not exported source metadata.".into());
            let mut design = EngineDesign {
                layout,
                bank_angle,
                ..Default::default()
            };
            design.set_cylinders(meta.cylinders);
            design
        };
        design.bank_delay_ms = if layout == Layout::Inline { 0. } else { 0.6 };
        let mut scratch = Scratch {
            design,
            ..Default::default()
        };
        let cylinder_m3 = displacement * 0.001 / meta.cylinders as f32;
        let square_m = (4. * cylinder_m3 / std::f32::consts::PI).cbrt();
        let (bore, stroke) = match (p.bore_m, p.stroke_m) {
            (Some(b), Some(s)) => {
                let geometry_l =
                    std::f32::consts::PI * 0.25 * b * b * s * meta.cylinders as f32 * 1000.;
                if !geometry_l.is_finite() || (geometry_l / displacement - 1.).abs() > 0.02 {
                    return Err("Automation Variant bore/stroke contradict its capacity".into());
                }
                (b, s)
            }
            _ => {
                notes.push(
                    "Bore/stroke incomplete: square geometry estimated from verified capacity."
                        .into(),
                );
                (square_m, square_m)
            }
        };
        if !(0.005..=0.5).contains(&bore) || !(0.005..=0.5).contains(&stroke) {
            return Err("Automation bore/stroke outside physical solver limits".into());
        }
        let b = &mut scratch.build;
        b.bore_mm = bore * 1000.;
        b.stroke_mm = stroke * 1000.;
        b.compression = bounded(
            p.compression,
            b.compression,
            7.,
            14.,
            "Compression ratio",
            &mut notes,
        );
        b.cam = bounded(p.cam_profile, b.cam, 0., 1., "Cam profile", &mut notes);
        b.valves = component(p.valves, b.valves, "Valves per cylinder", &mut notes);
        b.block = component(
            match p.block_material.as_deref() {
                Some("EngBlockMat_Iron_Name") => Some(BlockMaterial::CastIron),
                Some(
                    "EngBlockMat_AluL_Name" | "EngBlockMat_AlSiHD_Name" | "EngBlockMat_Alu_Name",
                ) => Some(BlockMaterial::Aluminium),
                _ => None,
            },
            b.block,
            "Block material",
            &mut notes,
        );
        b.head = component(
            match p.head.as_deref() {
                Some("Head_PushRod_Name") => Some(Head::Pushrod),
                Some("Head_OHC_Name" | "Head_DirectOHC_Name") => Some(Head::Sohc),
                Some("Head_DOHC_Name") => Some(Head::Dohc),
                _ => None,
            },
            b.head,
            "Cylinder head",
            &mut notes,
        );
        if matches!(p.head.as_deref(), Some("Head_DirectOHC_Name")) {
            notes.push("Direct-acting OHC mapped to the solver's generic SOHC category.".into());
        }
        b.crank = component(
            p.crank.as_deref().and_then(|tag| {
                if tag.starts_with("CrankMat_Forged") {
                    Some(Crankshaft::Forged)
                } else if tag.starts_with("CrankMat_Billet") {
                    Some(Crankshaft::Billet)
                } else if tag.starts_with("CrankMat_Cast") {
                    Some(Crankshaft::Cast)
                } else {
                    None
                }
            }),
            b.crank,
            "Crank material",
            &mut notes,
        );
        b.fuel = component(
            match p.fuel_system.as_deref() {
                Some("FuelSys_Inj_Direct_Name") => Some(Fuel::DirectInjection),
                Some(
                    "FuelSys_Inj_SingEFI_Name"
                    | "FuelSys_Inj_MultiEFI_Name"
                    | "FuelSys_Inj_MechFI_Name",
                ) => Some(Fuel::PortInjection),
                Some(tag) if tag.starts_with("FuelSys_Carb_") => Some(Fuel::Carburettor),
                _ => None,
            },
            b.fuel,
            "Fuel system",
            &mut notes,
        );
        b.aspiration = if !turbo {
            Aspiration::Natural
        } else {
            match p.aspiration_setup.as_deref() {
                Some("Turbo_Twin_Name") => Aspiration::TwinTurbo,
                Some("Turbo_Single_Name") => Aspiration::Turbo,
                _ => {
                    notes.push("Turbo layout absent/unsupported: single turbo estimated.".into());
                    Aspiration::Turbo
                }
            }
        };
        if turbo {
            b.boost_bar = bounded(
                p.boost_setting,
                b.boost_bar,
                0.2,
                2.5,
                "Boost setting (bar approximation)",
                &mut notes,
            );
            notes.push("Boost scalar interpreted as nominal gauge bar; compressor map, spool inertia, intercooler and wastegate behavior remain estimated.".into());
        }
        b.vvt = component(
            match p.vvt.as_deref() {
                Some("VVT_None_Name") => Some(false),
                Some("VarValves_VVTSOHC_Name" | "VarValves_VVTDOHC_Name") => Some(true),
                _ => None,
            },
            b.vvt,
            "VVT",
            &mut notes,
        );
        b.headers = component(
            match p.headers.as_deref() {
                Some(
                    "Header_CastLog_Name" | "Header_CastMid_Name" | "Header_TurboShortLog_Name",
                ) => Some(Headers::CastManifold),
                Some(
                    "Header_Tubular_Name"
                    | "Header_TurboRaceTubular_Name"
                    | "Header_TubularRace_Name",
                ) => Some(Headers::Tubular),
                _ => None,
            },
            b.headers,
            "Headers",
            &mut notes,
        );
        b.catalyst = component(
            match p.catalyst.as_deref() {
                Some("CatConvert_None_Name") => Some(Catalyst::None),
                Some("CatConvert_High3Way_Name") => Some(Catalyst::HighFlow),
                Some(
                    "CatConvert_2Way_Name" | "CatConvert_3Way_Name" | "CatConvert_3WayPreCat_Name",
                ) => Some(Catalyst::Standard),
                _ => None,
            },
            b.catalyst,
            "Catalyst",
            &mut notes,
        );
        let muffler = |tag: Option<&str>| match tag {
            Some("Muffler_None_Name") => Some((0, Muffler::None)),
            Some("Muffler_Straight_Name") => Some((1, Muffler::StraightThrough)),
            Some("Muffler_Confused_Name") => Some((2, Muffler::Baffled)),
            Some("Muffler_Reverse_Name") => Some((3, Muffler::ReverseFlow)),
            _ => None,
        };
        b.muffler = component(
            muffler(p.muffler_1.as_deref())
                .zip(muffler(p.muffler_2.as_deref()))
                .map(|(a, b)| if a.0 >= b.0 { a.1 } else { b.1 }),
            b.muffler,
            "Mufflers",
            &mut notes,
        );
        notes.push("Two source mufflers collapsed into one equivalent of the more restrictive class; dimensions and packing estimated.".into());
        b.exhaust_mm = (35. + 8. * displacement + if turbo { 8. } else { 0. }).clamp(35., 100.);
        notes.push(format!("Exhaust diameter estimated {:.1} mm from capacity; source ExhaustDiameter {:?} retained as raw value because its unit is unconfirmed.", b.exhaust_mm, p.exhaust_diameter));
        notes.push("Primary/tail lengths, bank delay, connecting-rod ratio, valve diameter/lift/timing law, thermal transfer, spark timing and rotating inertia are physical-model estimates, not recovered source measurements.".into());
        notes.push("Throttle/plenum, exhaust-bank routing/crossover and turbo blow-off layout use model defaults; source outlet count does not identify cylinder-bank routing.".into());
        scratch.apply_design();
        scratch.derive_from_build(
            &mut Settings::default(),
            &mut Parameters::default(),
            &mut Controls::default(),
        );
        scratch.idle_rpm = bounded(
            p.idle_rpm,
            if bank_min.is_finite() { bank_min } else { 850. },
            300.,
            2000.,
            "Idle RPM (recording minimum fallback)",
            &mut notes,
        );
        scratch.redline_rpm = bounded(
            p.rpm_limit,
            if bank_max.is_finite() {
                bank_max
            } else {
                7000.
            },
            scratch.idle_rpm + 1000.,
            12000.,
            "Rev limit (recording maximum fallback)",
            &mut notes,
        );
        scratch.validate()?;
        let baseline = EngineDefinition::from_scratch(&scratch);
        Ok(Self {
            baseline,
            provenance: provenance(meta, &baseline, &notes)?,
            scratch,
            assumptions: notes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physical::engine::{Commands, Engine};
    use std::{collections::HashSet, path::Path};

    #[test]
    fn essential_metadata_is_required_and_estimates_are_explicit() {
        let mut meta = EngineMeta {
            uid: "0123456789ABCDEF0123456789ABCDEF".into(),
            cylinders: 4,
            layout: SourceLayout::Inline,
            displacement_l: Some(2.),
            exhaust_count: None,
            turbocharged: Some(false),
            physical: Default::default(),
        };
        let inferred = AutomationModel::from_meta(&meta, 100., 20000.).unwrap();
        assert_eq!(inferred.scratch.idle_rpm, 300.);
        assert_eq!(inferred.scratch.redline_rpm, 12000.);
        assert!((inferred.scratch.build.cylinder_litres() * 4. - 2.).abs() < 1e-5);
        assert!(
            inferred
                .assumptions
                .iter()
                .any(|a| a.contains("square geometry"))
        );
        assert!(
            inferred
                .assumptions
                .iter()
                .any(|a| a.contains("20000 limited"))
        );
        meta.displacement_l = None;
        assert!(AutomationModel::from_meta(&meta, 850., 6000.).is_err());
        meta.displacement_l = Some(2.);
        meta.turbocharged = None;
        assert!(AutomationModel::from_meta(&meta, 850., 6000.).is_err());
        meta.turbocharged = Some(false);
        meta.physical.bore_m = Some(0.05);
        meta.physical.stroke_m = Some(0.05);
        assert!(AutomationModel::from_meta(&meta, 850., 6000.).is_err());
    }

    #[test]
    fn unified_import_tracks_estimates_edits_and_section_reset_without_reimport() {
        let meta = EngineMeta {
            uid: "0123456789ABCDEF0123456789ABCDEF".into(),
            cylinders: 4,
            layout: SourceLayout::Inline,
            displacement_l: Some(2.),
            exhaust_count: Some(1),
            turbocharged: Some(false),
            physical: crate::engine_meta::PhysicalMeta {
                compression: Some(9.5),
                exhaust_diameter: Some(2.5),
                ..Default::default()
            },
        };
        let model = AutomationModel::from_meta(&meta, 850., 6500.).unwrap();
        let status = |model: &AutomationModel, field: &str| {
            model
                .provenance
                .iter()
                .find(|item| item.field == field)
                .unwrap()
                .origin
        };
        assert_eq!(status(&model, "build.compression"), ValueOrigin::Read);
        assert_eq!(status(&model, "build.exhaust_mm"), ValueOrigin::Estimated);
        assert_eq!(status(&model, "tuning.cam.lift_mm"), ValueOrigin::Estimated);
        let mut edited = model.baseline;
        edited.build.compression = 11.;
        edited.sound.bass_db = 3.;
        edited.tuning.cam.lift_mm = Some(11.);
        let active = model.with_definition(&edited).unwrap();
        assert_eq!(active.baseline, model.baseline);
        assert_eq!(status(&active, "build.compression"), ValueOrigin::Modified);
        assert_eq!(status(&active, "tuning.cam.lift_mm"), ValueOrigin::Modified);
        edited.reset_section(
            &model.baseline,
            crate::engine_definition::EngineSection::Parts,
        );
        let reset = active.with_definition(&edited).unwrap();
        assert_eq!(status(&reset, "build.compression"), ValueOrigin::Read);
        assert_eq!(status(&reset, "sound.bass_db"), ValueOrigin::Modified);
        assert_eq!(reset.scratch.sound.bass_db, 3.);
        edited.reset_section(
            &model.baseline,
            crate::engine_definition::EngineSection::Tuning,
        );
        let reset_tuning = reset.with_definition(&edited).unwrap();
        assert_eq!(
            status(&reset_tuning, "tuning.cam.lift_mm"),
            ValueOrigin::Estimated
        );
        let json = serde_json::to_value(edited).unwrap();
        assert!(json.get("tuning").is_none());
        let reloaded: EngineDefinition = serde_json::from_value(json).unwrap();
        assert_eq!(reloaded, edited);
        let reloaded_model = reset_tuning.with_definition(&reloaded).unwrap();
        assert_eq!(
            status(&reloaded_model, "tuning.cam.lift_mm"),
            ValueOrigin::Estimated
        );
    }

    #[test]
    fn unified_resolve_preserves_legacy_sound_and_never_bypasses_source_identity() {
        let path = crate::test_support::automation_fixture();
        let mut bank = Bank::load(&path, None).unwrap();
        std::fs::remove_file(path).unwrap();
        let sound = SoundTuning {
            presence_db: -2.,
            ..Default::default()
        };
        let migrated = AutomationModel::resolve(&bank, None, &sound).unwrap();
        assert_eq!(migrated.scratch.sound, sound);
        assert_eq!(migrated.baseline.sound, SoundTuning::default());
        let mut active = EngineDefinition::from_scratch(&migrated.scratch);
        active.tuning.cam.lift_mm = Some(11.);
        active.sound.presence_db = 4.;
        let resolved = AutomationModel::resolve(&bank, Some(&active), &sound).unwrap();
        assert_eq!(EngineDefinition::from_scratch(&resolved.scratch), active);
        let mut saved_baseline = migrated.baseline;
        saved_baseline.build.exhaust_mm = 40.;
        let settings = Settings {
            engine: Some(active),
            engine_baseline: Some(saved_baseline),
            physical_sound: sound,
            ..Default::default()
        };
        let historical = AutomationModel::from_settings(&bank, &settings).unwrap();
        assert_eq!(historical.baseline, saved_baseline);
        assert_eq!(EngineDefinition::from_scratch(&historical.scratch), active);
        let restored = historical.with_definition(&saved_baseline).unwrap();
        assert!(
            restored
                .provenance
                .iter()
                .all(|item| item.origin != ValueOrigin::Modified)
        );
        bank.engine_meta.as_mut().unwrap().uid = "wrong-source".into();
        assert!(
            AutomationModel::resolve(&bank, Some(&active), &sound)
                .unwrap_err()
                .contains("UID")
        );
        assert!(AutomationModel::from_settings(&bank, &settings).is_err());
    }

    #[test]
    fn twelve_source_models_keep_identity_capacity_and_distinct_finite_sound() {
        let cars = Path::new(env!("CARGO_MANIFEST_DIR")).join("cars");
        if !cars.exists() {
            return;
        }
        let mut signatures = HashSet::new();
        let mut sounds = HashSet::new();
        let mut count = 0;
        for entry in std::fs::read_dir(cars).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|e| e.to_str()) != Some("zip") {
                continue;
            }
            let mut bank = Bank::load(&path, None).unwrap();
            if count == 0 {
                let original_range = (bank.min_rpm, bank.max_rpm);
                for (min, max) in [
                    (850., 20000.),
                    (-1., 6000.),
                    (7000., 6000.),
                    (f32::NAN, 6000.),
                    (850., f32::INFINITY),
                ] {
                    bank.min_rpm = min;
                    bank.max_rpm = max;
                    assert!(
                        AutomationModel::from_bank(&bank)
                            .unwrap_err()
                            .contains("RPM")
                    );
                }
                (bank.min_rpm, bank.max_rpm) = original_range;
            }
            let model = AutomationModel::from_bank(&bank).unwrap();
            let meta = bank.engine_meta.as_ref().unwrap();
            assert_eq!(model.scratch.design.cylinders, meta.cylinders);
            assert!(
                (model.scratch.build.cylinder_litres() * meta.cylinders as f32
                    / meta.displacement_l.unwrap()
                    - 1.)
                    .abs()
                    < 1e-5
            );
            assert_eq!(
                model.scratch.build.aspiration != Aspiration::Natural,
                meta.turbocharged.unwrap()
            );
            assert_eq!(model.scratch.redline_rpm, meta.physical.rpm_limit.unwrap());
            assert!(model.assumptions.iter().any(|a| a.contains("Firing order")));
            signatures.insert(serde_json::to_string(&model.scratch).unwrap());
            let mut engine = Engine::new(&model.scratch, 48000).unwrap();
            let mut hash = 0_u64;
            for frame in 0..24000 {
                let output = engine.next(Commands {
                    imposed_rpm: Some(2000.),
                    throttle: 0.35,
                    ..Default::default()
                });
                assert!(!engine.failed(), "{}", path.display());
                assert!(
                    [output.exhaust, output.intake, output.mechanical]
                        .iter()
                        .all(|v| v.is_finite())
                );
                if frame > 12000 {
                    hash = hash.rotate_left(1) ^ u64::from(output.exhaust.to_bits());
                }
            }
            sounds.insert(hash);
            println!(
                "{}: {} cylinders, {:.3}L, {:?}, idle{} limit{}, {} assumptions",
                path.file_name().unwrap().to_string_lossy(),
                meta.cylinders,
                meta.displacement_l.unwrap(),
                model.scratch.build.aspiration,
                model.scratch.idle_rpm,
                model.scratch.redline_rpm,
                model.assumptions.len()
            );
            count += 1;
        }
        assert_eq!(count, 12);
        assert_eq!(signatures.len(), count);
        assert_eq!(sounds.len(), count);
    }
}
