//! Bounded, data-only import of a uniquely selected BeamNG vehicle setup.
//! This is not a JBeam evaluator: expressions, conflicting configurations and
//! values that cannot be attributed to selected parts retain bench estimates.
use crate::drive::Controls;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs::File,
    io::Read,
    path::Path,
};

const FILE_LIMIT: u64 = 4_000_000;
const TOTAL_LIMIT: u64 = 64_000_000;
const ENTRY_LIMIT: usize = 20_000;
const NODE_LIMIT: usize = 1_000_000;
const DEPTH_LIMIT: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VehicleSetup {
    pub config: Option<String>,
    pub mass_kg: Option<f32>,
    pub gear_ratios: Vec<f32>,
    pub final_drive: Option<f32>,
    pub wheel_radius: Option<f32>,
    /// Declared engine peak torque used only to size the bench clutch.
    /// The simulated engine supplies the actual driving torque.
    pub clutch_torque_nm: Option<f32>,
    pub notes: Vec<String>,
}
impl VehicleSetup {
    /// Apply only resolved, in-range values, preserving every missing estimate.
    pub fn apply(&self, controls: &mut Controls) {
        for (source, target, min, max) in [
            (self.mass_kg, &mut controls.mass_kg, 300., 6000.),
            (self.final_drive, &mut controls.final_drive, 0.5, 15.),
            (self.wheel_radius, &mut controls.wheel_radius, 0.2, 0.6),
            (
                self.clutch_torque_nm,
                &mut controls.peak_torque_nm,
                30.,
                2000.,
            ),
        ] {
            if let Some(value) = source.filter(|v| v.is_finite() && (min..=max).contains(v)) {
                *target = value;
            }
        }
        if valid_gears(&self.gear_ratios) {
            controls.gear_count = self.gear_ratios.len() as u8;
            for (i, ratio) in self.gear_ratios.iter().enumerate() {
                controls.set_ratio(i, *ratio);
            }
            controls.gear = controls.gear.min(controls.gear_count);
        }
    }
    fn missing_notes(&mut self) {
        for (missing, label) in [
            (self.mass_kg.is_none(), "Declared vehicle mass"),
            (self.gear_ratios.is_empty(), "Forward gearbox ratios"),
            (self.final_drive.is_none(), "Final drive"),
            (self.wheel_radius.is_none(), "Tyre radius"),
            (self.clutch_torque_nm.is_none(), "Clutch torque reference"),
        ] {
            if missing {
                self.notes.push(format!(
                    "{label} unresolved; listening-bench estimate retained."
                ));
            }
        }
    }
}

/// Inspect without extracting, executing expressions, or modifying the source.
pub fn inspect(path: &Path) -> Result<VehicleSetup, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > ENTRY_LIMIT {
        return Err("Too many files in the vehicle archive".into());
    }
    let mut documents = BTreeMap::new();
    let mut seen_names = BTreeSet::new();
    let mut total = 0;
    let mut notes = Vec::new();
    let mut config_entries = 0;
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_owned();
        let basename = name.rsplit('/').next().unwrap_or("");
        if !name.starts_with("vehicles/")
            || !(name.ends_with(".pc")
                || name.ends_with(".jbeam")
                || (basename.starts_with("info_") && name.ends_with(".json")))
        {
            continue;
        }
        if name.ends_with(".pc") {
            config_entries += 1;
        }
        if entry.size() > FILE_LIMIT || entry.size() > TOTAL_LIMIT - total {
            return Err(format!("Vehicle metadata read limit exceeded: {name}"));
        }
        total += entry.size();
        let mut text = String::new();
        entry
            .take(FILE_LIMIT + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        if text.len() as u64 > FILE_LIMIT {
            return Err("Vehicle metadata read limit exceeded".into());
        }
        if !seen_names.insert(name.clone()) {
            documents.remove(&name);
            notes.push(format!("Duplicate archive metadata ignored: {name}"));
            continue;
        }
        match Parser::parse(&text) {
            Ok(value) => {
                documents.insert(name, value);
            }
            Err(error) => notes.push(format!("Unsupported metadata ignored: {name}: {error}")),
        }
    }
    let mut result = if config_entries == 1 {
        resolve(&documents)
    } else {
        VehicleSetup {
            notes: vec![
                "A unique vehicle .pc configuration is required; no configuration guessed.".into(),
            ],
            ..Default::default()
        }
    };
    result.notes.splice(0..0, notes);
    result.missing_notes();
    Ok(result)
}

struct Part<'a> {
    file: &'a str,
    data: &'a Value,
}

fn resolve(documents: &BTreeMap<String, Value>) -> VehicleSetup {
    let mut result = VehicleSetup::default();
    let configs: Vec<_> = documents
        .iter()
        .filter(|(name, _)| name.ends_with(".pc"))
        .collect();
    if configs.len() != 1 {
        result.notes.push(
            "A unique vehicle .pc configuration is required; no configuration guessed.".into(),
        );
        return result;
    }
    let (config_name, pc) = configs[0];
    result.config = Some(config_name.clone());
    let Some((folder, config_file)) = config_name.rsplit_once('/') else {
        return result;
    };
    let root = folder.split('/').take(2).collect::<Vec<_>>().join("/") + "/";
    let info_name = format!("{folder}/info_{}.json", config_file.trim_end_matches(".pc"));
    if let Some(info) = documents.get(&info_name) {
        result.mass_kg = literal(&info["Weight"]).filter(|v| (300.0..=6000.).contains(v));
        result.clutch_torque_nm = literal(&info["Torque"]).filter(|v| (30.0..=2000.).contains(v));
        if result.mass_kg.is_some() {
            result.notes.push(format!(
                "Mass: {info_name}: Weight (declared total, not a node sum)."
            ));
        }
        if result.clutch_torque_nm.is_some() {
            result.notes.push(format!(
                "Clutch reference: {info_name}: Torque; actual driving torque remains physical."
            ));
        }
    }
    let Some(selected) = pc.get("parts").and_then(Value::as_object) else {
        result
            .notes
            .push(format!("{config_name} has no usable parts selection."));
        return result;
    };
    let mut parts: BTreeMap<&str, Vec<Part<'_>>> = BTreeMap::new();
    for (file, value) in documents
        .iter()
        .filter(|(n, _)| n.starts_with(&root) && n.ends_with(".jbeam"))
    {
        if let Some(object) = value.as_object() {
            for (name, data) in object {
                parts.entry(name).or_default().push(Part { file, data });
            }
        }
    }
    // Walk from the unique main part: old/detached choices left in the PC are
    // not active unless a reachable slot selects them. Empty removes a slot.
    let roots: Vec<_> = parts
        .iter()
        .filter(|(_, p)| p.len() == 1 && p[0].data["slotType"] == "main")
        .map(|(name, _)| *name)
        .collect();
    if roots.len() != 1 {
        result.notes.push("A unique main JBeam part is required to prove the active drivetrain; bench estimates retained.".into());
        return result;
    }
    let mut pending: VecDeque<String> = [roots[0].to_owned()].into();
    let mut active = BTreeSet::new();
    let mut ambiguous_slots: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    while let Some(name) = pending.pop_front() {
        if active.contains(&name) {
            continue;
        }
        let Some(candidates) = parts.get(name.as_str()) else {
            continue;
        };
        if candidates.len() != 1 {
            result.notes.push(format!(
                "Selected part {name} has multiple definitions; ignored."
            ));
            continue;
        }
        active.insert(name);
        for (slot, default) in table_pairs(&candidates[0].data["slots"], "type", "default") {
            let chosen = selected
                .get(slot)
                .and_then(Value::as_str)
                .unwrap_or(default);
            if chosen.is_empty() {
                continue;
            }
            if let Some(candidates) = parts.get(chosen)
                && candidates.len() == 1
                && candidates[0].data.get("slotType").and_then(Value::as_str) != Some(slot)
            {
                result.notes.push(format!(
                    "Active slot {slot}: part {chosen} has a mismatched or unsupported slotType; ignored."
                ));
                continue;
            }
            ambiguous_slots
                .entry(slot.to_owned())
                .or_default()
                .insert(chosen.to_owned());
            pending.push_back(chosen.to_owned());
        }
    }
    if ambiguous_slots.values().any(|choices| choices.len() > 1) {
        result
            .notes
            .push("Conflicting active slot defaults; drivetrain values left estimated.".into());
        return result;
    }
    let mut variables: BTreeMap<String, Option<f32>> = BTreeMap::new();
    for name in &active {
        let part = &parts[name.as_str()][0];
        if let Some(rows) = part.data["variables"].as_array() {
            let header = rows.first().and_then(Value::as_array);
            let name_col = column(header, "name");
            let default_col = column(header, "default");
            if let (Some(n), Some(d)) = (name_col, default_col) {
                for row in rows.iter().skip(1).filter_map(Value::as_array) {
                    if let Some(name) = row.get(n).and_then(Value::as_str) {
                        let value = row.get(d).and_then(literal);
                        variables
                            .entry(name.into())
                            .and_modify(|old| {
                                if *old != value {
                                    *old = None;
                                }
                            })
                            .or_insert(value);
                    }
                }
            }
        }
    }
    if let Some(overrides) = pc.get("vars").and_then(Value::as_object) {
        for (name, value) in overrides {
            variables.insert(name.clone(), literal(value));
        }
    }
    let mut gears = Vec::new();
    let mut drives = Vec::new();
    let mut radii = Vec::new();
    for name in &active {
        let part = &parts[name.as_str()][0];
        let source = format!("{} / {name}", part.file);
        if let Some(value) = part.data.get("gearbox").and_then(|v| v.get("gearRatios")) {
            gears.push((forward_gears(value, &variables), source.clone()));
        }
        // Named differential devices only; gearbox/transfer-case reductions
        // and unrelated gearRatio fields must not become the final drive.
        let mut differential_names: BTreeSet<&str> =
            ["frontDiff", "rearDiff"].into_iter().collect();
        for (kind, name) in table_pairs(&part.data["powertrain"], "type", "name") {
            if kind == "differential" {
                differential_names.insert(name);
            }
        }
        for device in differential_names {
            if let Some(value) = part.data.get(device).and_then(|v| v.get("gearRatio")) {
                drives.push((
                    number(value, &variables).filter(|v| (0.5..=15.).contains(v)),
                    source.clone(),
                ));
            }
        }
        if let Some(rows) = part.data.get("pressureWheels").and_then(Value::as_array) {
            for row in rows {
                // JBeam table option objects apply to subsequent wheel rows;
                // conflicting radii cannot safely become one bench radius.
                if let Some(value) = row.get("radius") {
                    radii.push((
                        number(value, &variables).filter(|v| (0.2..=0.6).contains(v)),
                        source.clone(),
                    ));
                }
                if let Some(cells) = row.as_array() {
                    for cell in cells {
                        if let Some(value) = cell.get("radius") {
                            radii.push((
                                number(value, &variables).filter(|v| (0.2..=0.6).contains(v)),
                                source.clone(),
                            ));
                        }
                    }
                }
            }
        }
    }
    if gears.len() == 1 {
        if let Some(values) = gears[0].0.take() {
            result.gear_ratios = values;
            result.notes.push(format!(
                "Forward ratios: {} (PC variables override active-part defaults).",
                gears[0].1
            ));
        } else {
            result.notes.push("Selected forward ratios contain unresolved expressions/variables or invalid ordering.".into());
        }
    } else if !gears.is_empty() {
        result
            .notes
            .push("Multiple selected gearboxes; forward ratios left estimated.".into());
    }
    result.final_drive = consensus(&drives);
    result.wheel_radius = consensus(&radii);
    for (value, candidates, label) in [
        (result.final_drive, &drives, "Final drive"),
        (result.wheel_radius, &radii, "Tyre radius"),
    ] {
        if value.is_some() {
            let sources: BTreeSet<_> = candidates.iter().map(|(_, s)| s.as_str()).collect();
            result.notes.push(format!(
                "{label}: {}.",
                sources.into_iter().collect::<Vec<_>>().join("; ")
            ));
        } else if !candidates.is_empty() {
            result.notes.push(format!(
                "{label}: selected values disagree, are unsupported or out of bounds."
            ));
        }
    }
    result
}
fn literal(value: &Value) -> Option<f32> {
    value.as_f64().map(|n| n as f32).filter(|n| n.is_finite())
}
fn number(value: &Value, variables: &BTreeMap<String, Option<f32>>) -> Option<f32> {
    literal(value).or_else(|| {
        let name = value.as_str()?;
        (name.starts_with('$') && !name.starts_with("$="))
            .then(|| variables.get(name).copied().flatten())
            .flatten()
    })
}
fn valid_gears(values: &[f32]) -> bool {
    (1..=12).contains(&values.len())
        && values
            .iter()
            .all(|n| n.is_finite() && (0.05..=20.).contains(n))
        && values.windows(2).all(|p| p[0] > p[1])
}
fn forward_gears(value: &Value, variables: &BTreeMap<String, Option<f32>>) -> Option<Vec<f32>> {
    let all = value.as_array()?;
    // The neutral marker separates reverse (which can be an unevaluated $=
    // expression) from forward ratios. No forward entry may be silently lost.
    let neutral: Vec<_> = all
        .iter()
        .enumerate()
        .filter(|(_, n)| number(n, variables) == Some(0.))
        .map(|(i, _)| i)
        .collect();
    if neutral.len() != 1 {
        return None;
    }
    let forward = all[neutral[0] + 1..]
        .iter()
        .map(|n| number(n, variables))
        .collect::<Option<Vec<_>>>()?;
    valid_gears(&forward).then_some(forward)
}
fn consensus(values: &[(Option<f32>, String)]) -> Option<f32> {
    let first = values.first()?.0?;
    values
        .iter()
        .all(|(v, _)| *v == Some(first))
        .then_some(first)
}
fn column(header: Option<&Vec<Value>>, name: &str) -> Option<usize> {
    header?.iter().position(|v| v.as_str() == Some(name))
}
fn table_pairs<'a>(table: &'a Value, key: &str, value: &str) -> Vec<(&'a str, &'a str)> {
    let Some(rows) = table.as_array() else {
        return Vec::new();
    };
    let header = rows.first().and_then(Value::as_array);
    let (Some(k), Some(v)) = (column(header, key), column(header, value)) else {
        return Vec::new();
    };
    rows.iter()
        .skip(1)
        .filter_map(Value::as_array)
        .filter_map(|row| Some((row.get(k)?.as_str()?, row.get(v)?.as_str()?)))
        .collect()
}

/// JSON values plus JBeam comments, trailing commas and omitted commas.
/// Rejects code, bare identifiers, duplicate keys, excess depth and node count.
struct Parser<'a> {
    text: &'a str,
    pos: usize,
    nodes: usize,
}
impl<'a> Parser<'a> {
    fn parse(text: &'a str) -> Result<Value, String> {
        let mut parser = Self {
            text: text.trim_start_matches('\u{feff}'),
            pos: 0,
            nodes: 0,
        };
        let value = parser.value(0)?;
        parser.skip()?;
        if parser.pos != parser.text.len() {
            return Err("trailing data".into());
        }
        Ok(value)
    }
    fn skip(&mut self) -> Result<(), String> {
        loop {
            while self
                .text
                .as_bytes()
                .get(self.pos)
                .is_some_and(u8::is_ascii_whitespace)
            {
                self.pos += 1;
            }
            if self.text[self.pos..].starts_with("//") {
                self.pos += self.text[self.pos..]
                    .find('\n')
                    .unwrap_or(self.text.len() - self.pos);
            } else if self.text[self.pos..].starts_with("/*") {
                let Some(end) = self.text[self.pos + 2..].find("*/") else {
                    return Err("unterminated comment".into());
                };
                self.pos += end + 4;
            } else {
                return Ok(());
            }
        }
    }
    fn string(&mut self) -> Result<String, String> {
        let start = self.pos;
        self.pos += 1;
        let bytes = self.text.as_bytes();
        while self.pos < bytes.len() {
            match bytes[self.pos] {
                b'\\' => self.pos += 2,
                b'"' => {
                    self.pos += 1;
                    return serde_json::from_str(&self.text[start..self.pos])
                        .map_err(|e| e.to_string());
                }
                _ => self.pos += 1,
            }
        }
        Err("unterminated string".into())
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        self.nodes += 1;
        if depth > DEPTH_LIMIT || self.nodes > NODE_LIMIT {
            return Err("parser resource limit".into());
        }
        self.skip()?;
        let Some(&first) = self.text.as_bytes().get(self.pos) else {
            return Err("missing value".into());
        };
        if first == b'"' {
            return self.string().map(Value::String);
        }
        if first == b'{' || first == b'[' {
            let object = first == b'{';
            let end = if object { b'}' } else { b']' };
            self.pos += 1;
            let mut map = Map::new();
            let mut array = Vec::new();
            loop {
                self.skip()?;
                if self.text.as_bytes().get(self.pos) == Some(&end) {
                    self.pos += 1;
                    break;
                }
                if object {
                    if self.text.as_bytes().get(self.pos) != Some(&b'"') {
                        return Err("object key must be quoted".into());
                    }
                    let key = self.string()?;
                    self.skip()?;
                    if self.text.as_bytes().get(self.pos) != Some(&b':') {
                        return Err("missing colon".into());
                    }
                    self.pos += 1;
                    let value = self.value(depth + 1)?;
                    if map.insert(key, value).is_some() {
                        return Err("duplicate object key".into());
                    }
                } else {
                    array.push(self.value(depth + 1)?);
                }
                self.skip()?;
                if self.text.as_bytes().get(self.pos) == Some(&b',') {
                    self.pos += 1;
                }
            }
            return Ok(if object {
                Value::Object(map)
            } else {
                Value::Array(array)
            });
        }
        let start = self.pos;
        while self
            .text
            .as_bytes()
            .get(self.pos)
            .is_some_and(|c| !c.is_ascii_whitespace() && !b",]}[/{}:\"".contains(c))
        {
            self.pos += 1;
        }
        if self.pos == start {
            return Err("unsupported token".into());
        }
        serde_json::from_str(&self.text[start..self.pos]).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const PC: &str =
        r#"{"parts":{"transmission":"selected","wheels":"wheels"},"vars":{"$first":3.2}}"#;
    const PARTS: &str = r#"{
        "main": {"slotType":"main", "slots":[["type","default","description"] ["transmission","selected","Gearbox"] ["wheels","wheels","Wheels"] ["other","","Other"] ["front","","Front"]]},
        "selected": {
            "slotType":"transmission",
            "slots":[["type","default","description"] ["finaldrive","selected_fd","Final"]],
            "variables":[["name","type","default"] ["$first","range",3.5] ["$second","range",2.1]],
            "gearbox":{"gearRatios":["$=-$reverse",0,"$first","$second",1.5,1.1,0.9,0.7,0.6]}
        },
        "unused": {"slotType":"transmission","gearbox":{"gearRatios":[-2,0,4,3,2]}},
        "second_gearbox": {"slotType":"other","gearbox":{"gearRatios":[-2,0,4,3,2]}},
        "selected_fd":{"slotType":"finaldrive","rearDiff":{"gearRatio":4.2}},
        "unused_fd":{"slotType":"finaldrive","rearDiff":{"gearRatio":9}},
        "front_fd":{"slotType":"front","frontDiff":{"gearRatio":9}},
        "wheels":{"slotType":"wheels","pressureWheels":[["name"] {"radius":0.34}, ["FL"], ["FR"]]},
    }"#;

    fn docs(pc: &str, parts: &str) -> BTreeMap<String, Value> {
        [
            ("vehicles/test/base.pc".into(), Parser::parse(pc).unwrap()),
            (
                "vehicles/test/parts.jbeam".into(),
                Parser::parse(parts).unwrap(),
            ),
            (
                "vehicles/test/info_base.json".into(),
                serde_json::json!({"Weight":1500,"Torque":410}),
            ),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn selected_parts_variables_and_slot_defaults_are_used_without_unused_parts() {
        let setup = resolve(&docs(PC, PARTS));
        assert_eq!(setup.mass_kg, Some(1500.));
        assert_eq!(setup.clutch_torque_nm, Some(410.));
        assert_eq!(setup.gear_ratios, vec![3.2, 2.1, 1.5, 1.1, 0.9, 0.7, 0.6]);
        assert_eq!(setup.final_drive, Some(4.2));
        assert_eq!(setup.wheel_radius, Some(0.34));
        let mut controls = Controls::default();
        setup.apply(&mut controls);
        assert_eq!(controls.gear_count, 7);
        assert_eq!(controls.ratio(6), 0.6);
        assert_eq!(controls.mass_kg, 1500.);
        controls.validate().unwrap();
        assert!(
            setup
                .notes
                .iter()
                .any(|s| s.contains("parts.jbeam / selected"))
        );
    }

    #[test]
    fn pc_slot_override_and_removal_defeat_active_default() {
        let pc = r#"{"parts":{"transmission":"selected","finaldrive":"unused_fd"}}"#;
        assert_eq!(resolve(&docs(pc, PARTS)).final_drive, Some(9.));
        let pc = r#"{"parts":{"transmission":"selected","finaldrive":""}}"#;
        assert_eq!(resolve(&docs(pc, PARTS)).final_drive, None);
        let pc = r#"{"parts":{"transmission":"selected","detached":"unused_fd"}}"#;
        assert_eq!(resolve(&docs(pc, PARTS)).final_drive, Some(4.2));
    }

    #[test]
    fn a_gearbox_selected_in_a_wheel_slot_cannot_supply_driving_ratios() {
        let pc = r#"{"parts":{"transmission":"","wheels":"unused"}}"#;
        let setup = resolve(&docs(pc, PARTS));
        assert!(setup.gear_ratios.is_empty());
        assert_eq!(setup.wheel_radius, None);
        assert!(
            setup
                .notes
                .iter()
                .any(|n| n.contains("slot wheels") && n.contains("slotType"))
        );
        let mut controls = Controls::default();
        setup.apply(&mut controls);
        assert_eq!(controls.ratios, Controls::default().ratios);
    }

    #[test]
    fn expressions_and_unknown_forward_variables_do_not_drop_gear_positions() {
        for value in ["$=2+2", "$missing"] {
            let parts = PARTS.replace("\"$second\",1.5", &format!("\"{value}\",1.5"));
            let setup = resolve(&docs(PC, &parts));
            assert!(setup.gear_ratios.is_empty());
            let mut controls = Controls::default();
            let expected = controls.ratios;
            setup.apply(&mut controls);
            assert_eq!(controls.ratios, expected);
        }
        let pc = r#"{"parts":{"transmission":"selected"},"vars":{"$first":"$=3+1"}}"#;
        assert!(resolve(&docs(pc, PARTS)).gear_ratios.is_empty());
    }

    #[test]
    fn ambiguous_configurations_devices_and_duplicate_parts_retain_estimates() {
        let mut documents = docs(PC, PARTS);
        documents.insert("vehicles/test/other.pc".into(), Parser::parse(PC).unwrap());
        let setup = resolve(&documents);
        assert!(setup.config.is_none() && setup.mass_kg.is_none() && setup.gear_ratios.is_empty());
        let pc =
            r#"{"parts":{"transmission":"selected","other":"second_gearbox","front":"front_fd"}}"#;
        let setup = resolve(&docs(pc, PARTS));
        assert!(setup.gear_ratios.is_empty());
        assert_eq!(setup.final_drive, None);
        let mut documents = docs(PC, PARTS);
        documents.insert(
            "vehicles/test/duplicate.jbeam".into(),
            serde_json::json!({"selected":{"gearbox":{"gearRatios":[-1,0,4,2]}}}),
        );
        assert!(resolve(&documents).gear_ratios.is_empty());
    }

    #[test]
    fn only_declared_total_mass_is_read_and_unsafe_values_are_not_applied() {
        let mut documents = docs(PC, PARTS);
        documents.remove("vehicles/test/info_base.json");
        documents.get_mut("vehicles/test/parts.jbeam").unwrap()["selected"]["nodes"] =
            serde_json::json!([{"nodeWeight":1500}]);
        assert_eq!(resolve(&documents).mass_kg, None);
        let setup = VehicleSetup {
            mass_kg: Some(f32::NAN),
            gear_ratios: vec![2., 3.],
            final_drive: Some(100.),
            wheel_radius: Some(0.1),
            clutch_torque_nm: Some(0.),
            ..Default::default()
        };
        let mut controls = Controls::default();
        setup.apply(&mut controls);
        assert_eq!(controls, Controls::default());
    }

    #[test]
    fn data_parser_accepts_comments_but_rejects_code_depth_and_duplicate_keys() {
        let value = Parser::parse(
            r#"{/*x*/"a":[1 2,] // line
            "b":"//not a comment", "c":true}"#,
        )
        .unwrap();
        assert_eq!(value["a"], serde_json::json!([1, 2]));
        assert_eq!(value["b"], "//not a comment");
        for text in [
            r#"{"a":1,"a":2}"#,
            "/*unterminated",
            "os.execute('x')",
            "{\"a\":undefined}",
        ] {
            assert!(Parser::parse(text).is_err());
        }
        assert!(
            Parser::parse(&("[".repeat(DEPTH_LIMIT + 2) + "0" + &"]".repeat(DEPTH_LIMIT + 2)))
                .is_err()
        );
    }

    fn archive(files: &[(&str, &str)]) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        // Windows wall-clock timestamps can be identical in parallel tests.
        // Reserve each filename exclusively; never truncate another fixture.
        static NEXT_ARCHIVE: AtomicU64 = AtomicU64::new(0);
        let (path, file) = loop {
            let path = std::env::temp_dir().join(format!(
                "bess-setup-{}-{}-{}.zip",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
                NEXT_ARCHIVE.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("Could not create vehicle setup fixture: {error}"),
            }
        };
        let mut zip = zip::ZipWriter::new(file);
        for (name, data) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn malformed_second_config_cannot_make_the_first_configuration_unique() {
        let path = archive(&[
            ("vehicles/test/base.pc", PC),
            ("vehicles/test/invalid.pc", "{"),
        ]);
        let setup = inspect(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(setup.config.is_none());
        assert!(setup.notes.iter().any(|n| n.contains("unique")));
    }

    #[test]
    fn oversized_metadata_is_bounded_before_parsing() {
        let big = " ".repeat(FILE_LIMIT as usize + 1);
        let path = archive(&[("vehicles/test/base.pc", &big)]);
        let result = inspect(&path);
        std::fs::remove_file(path).unwrap();
        assert!(result.unwrap_err().contains("limit"));
    }
}
