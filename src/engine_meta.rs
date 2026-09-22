//! Conservative engine metadata from an Automation `.car` inside a vehicle ZIP.
//!
//! The `.car` begins with Lua table syntax and can contain binary image data later.
//! This module reads only the small, textual Family and Variant tables. It never
//! evaluates Lua and returns no metadata unless the selected audio blend, variant
//! UID, installed `.pc` engine part, and engine JBeam agree.

use serde::Serialize;
use std::{fs::File, io::Read, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Layout {
    Inline,
    V { bank_angle_degrees: u16 },
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct EngineMeta {
    /// Automation Variant.UID, matched to the selected blend filename.
    pub uid: String,
    pub cylinders: u32,
    pub layout: Layout,
    /// Automation variant capacity in litres, if it is a plausible literal.
    pub displacement_l: Option<f32>,
    /// Number of exhaust outlets declared by Automation, not cylinder banks.
    pub exhaust_count: Option<u8>,
    /// Whether Automation names turbo aspiration; unknown types remain unknown.
    pub turbocharged: Option<bool>,
}

const MAX_ENTRIES: usize = 20_000;
const MAX_CAR_BYTES: u64 = 1_000_000;
const MAX_PC_BYTES: u64 = 100_000;
const MAX_JBEAM_BYTES: u64 = 1_000_000;
const MAX_TABLE_SCAN: usize = 64_000;
const BLEND_PREFIX: &str = "art/sound/blends/";
const BLEND_SUFFIX: &str = ".sfxBlend2D.json";

fn uid_from_blend(blend: &str) -> Option<&str> {
    let name = blend
        .strip_prefix(BLEND_PREFIX)?
        .strip_suffix(BLEND_SUFFIX)?;
    if name.len() == 32 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(name)
    } else {
        None
    }
}

fn table<'a>(data: &'a [u8], key: &str) -> Option<&'a [u8]> {
    if !data.starts_with(b"do local _={") {
        return None;
    }
    let marker = format!("\n\t{key}={{");
    let head = &data[..data.len().min(MAX_TABLE_SCAN)];
    let start = head
        .windows(marker.len())
        .position(|s| s == marker.as_bytes())?
        + marker.len()
        - 1;
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for (offset, &byte) in head[start..].iter().enumerate() {
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&head[start..=start + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

fn literal<'a>(table: &'a [u8], key: &str) -> Option<&'a str> {
    let text = std::str::from_utf8(table).ok()?;
    let prefix = format!("\t\t{key}=");
    let mut matches = text.lines().filter_map(|line| line.strip_prefix(&prefix));
    let value = matches.next()?.trim().trim_end_matches(',').trim();
    if matches.next().is_some() {
        return None;
    }
    Some(value)
}

fn string_literal(table: &[u8], key: &str) -> Option<String> {
    serde_json::from_str(literal(table, key)?).ok()
}

fn hex_uid(value: String) -> Option<String> {
    if value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
        Some(value.to_ascii_uppercase())
    } else {
        None
    }
}

fn architecture(config: &str, block_type: &str) -> Option<(u32, Layout)> {
    if let Some(count) = config
        .strip_prefix("EngBlock_Inl")
        .and_then(|s| s.strip_suffix("_Name"))
        .and_then(|s| s.parse::<u32>().ok())
        .filter(|n| (1..=12).contains(n))
    {
        return (block_type == "EngBlock_Inl_Name").then_some((count, Layout::Inline));
    }
    let body = config.strip_prefix("EngBlock_V")?.strip_suffix("_Name")?;
    let (count, angle_hint) = match body.split_once("_V") {
        Some((count, angle)) => (count, Some(angle)),
        None => (body, None),
    };
    let count = count.parse::<u32>().ok()?.min(13);
    let angle = block_type
        .strip_prefix("EngBlock_V")?
        .strip_suffix("_Name")?
        .parse::<u16>()
        .ok()?;
    if !(2..=12).contains(&count)
        || !(1..180).contains(&angle)
        || angle_hint.is_some_and(|hint| hint.parse::<u16>().ok() != Some(angle))
    {
        return None;
    }
    Some((
        count,
        Layout::V {
            bank_angle_degrees: angle,
        },
    ))
}

fn parse_car(data: &[u8]) -> Option<EngineMeta> {
    let family = table(data, "Family")?;
    let variant = table(data, "Variant")?;
    let family_uid = hex_uid(string_literal(family, "UID")?)?;
    let variant_uid = hex_uid(string_literal(variant, "UID")?)?;
    if hex_uid(string_literal(variant, "FUID")?)? != family_uid {
        return None;
    }
    let config = string_literal(family, "BlockConfig")?;
    let block_type = string_literal(family, "BlockType")?;
    let (cylinders, layout) = architecture(&config, &block_type)?;
    let displacement_l = literal(variant, "Capacity")
        .and_then(|s| s.parse::<f32>().ok())
        .filter(|n| n.is_finite() && (0.1..=20.).contains(n));
    let exhaust_count = string_literal(variant, "ExhaustCount")
        .and_then(|s| {
            s.strip_prefix("Exhausts_")?
                .strip_suffix("_Name")?
                .parse()
                .ok()
        })
        .filter(|n: &u8| (1..=4).contains(n));
    let turbocharged = match string_literal(variant, "AspirationType").as_deref() {
        Some("Aspiration_Natural_Name") => Some(false),
        Some("Aspiration_Turbo_Name") => Some(true),
        _ => None,
    };
    Some(EngineMeta {
        uid: variant_uid,
        cylinders,
        layout,
        displacement_l,
        exhaust_count,
        turbocharged,
    })
}

fn read_member(zip: &mut zip::ZipArchive<File>, name: &str, limit: u64) -> Option<Vec<u8>> {
    let member = zip.by_name(name).ok()?;
    if member.size() > limit {
        return None;
    }
    let mut data = Vec::new();
    member.take(limit + 1).read_to_end(&mut data).ok()?;
    (data.len() as u64 <= limit).then_some(data)
}

fn vehicle_root(car_name: &str) -> Option<String> {
    let rest = car_name.strip_prefix("vehicles/")?;
    let (vehicle, filename) = rest.split_once('/')?;
    if vehicle.is_empty() || filename.contains('/') || !filename.ends_with(".car") {
        return None;
    }
    Some(format!("vehicles/{vehicle}/"))
}

fn matches_active_engine(zip: &mut zip::ZipArchive<File>, root: &str, uid: &str) -> bool {
    let short = uid[..5].to_ascii_lowercase();
    let engine_path = format!("{root}eng_{short}/camso_engine_{short}.jbeam");
    let Some(engine_name) = zip
        .file_names()
        .find(|n| n.eq_ignore_ascii_case(&engine_path))
        .map(str::to_owned)
    else {
        return false;
    };
    let Some(jbeam) = read_member(zip, &engine_name, MAX_JBEAM_BYTES) else {
        return false;
    };
    // Confirm that the selected part is declared in this JBeam. Its commented
    // cylinder-frequency hint is inconsistent across the actual corpus.
    let head = String::from_utf8_lossy(&jbeam[..jbeam.len().min(256)]);
    let Some(body) = head.trim_start().strip_prefix('{').map(str::trim_start) else {
        return false;
    };
    let expected_key = format!("\"Camso_Engine_{short}\"");
    if !body
        .get(..expected_key.len())
        .is_some_and(|key| key.eq_ignore_ascii_case(&expected_key))
        || !body[expected_key.len()..].trim_start().starts_with(':')
    {
        return false;
    }
    let pcs: Vec<String> = zip
        .file_names()
        .filter(|n| {
            n.strip_prefix(root)
                .is_some_and(|rest| !rest.contains('/') && rest.ends_with(".pc"))
        })
        .map(str::to_owned)
        .collect();
    if pcs.len() != 1 {
        return false;
    }
    let Some(bytes) = read_member(zip, &pcs[0], MAX_PC_BYTES) else {
        return false;
    };
    let Ok(pc) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    pc["parts"]["Camso_Engine"]
        .as_str()
        .is_some_and(|part| part.eq_ignore_ascii_case(&format!("Camso_Engine_{short}")))
}

/// Read metadata only for the engine corresponding to `selected_blend`.
/// Missing, ambiguous or inconsistent metadata produces `None`.
pub fn inspect(zip_path: &Path, selected_blend: &str) -> Option<EngineMeta> {
    let blend_uid = uid_from_blend(selected_blend)?;
    let mut zip = zip::ZipArchive::new(File::open(zip_path).ok()?).ok()?;
    if zip.len() > MAX_ENTRIES || zip.by_name(selected_blend).is_err() {
        return None;
    }
    let car_names: Vec<String> = zip
        .file_names()
        .filter(|name| vehicle_root(name).is_some())
        .map(str::to_owned)
        .collect();
    let mut found = None;
    for car_name in car_names {
        let Some(data) = read_member(&mut zip, &car_name, MAX_CAR_BYTES) else {
            continue;
        };
        let Some(meta) = parse_car(&data) else {
            continue;
        };
        if !meta.uid.eq_ignore_ascii_case(blend_uid) {
            continue;
        }
        let root = vehicle_root(&car_name)?;
        if !matches_active_engine(&mut zip, &root, &meta.uid) || found.is_some() {
            return None;
        }
        found = Some(meta);
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::PathBuf};

    const UID: &str = "694E80154252F6189DE80988120C7F13";
    const BLEND: &str = "art/sound/blends/694E80154252F6189DE80988120C7F13.sfxBlend2D.json";

    fn fixture_with(
        car_uid: &str,
        selected: &str,
        aspiration: Option<&str>,
        jbeam_key: &str,
    ) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "bess-engine-meta-{}-{}.zip",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        let opt = zip::write::SimpleFileOptions::default();
        zip.start_file(BLEND, opt).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.start_file("vehicles/test/test.car", opt).unwrap();
        let aspiration = aspiration
            .map(|value| format!("\n\t\tAspirationType=\"{value}\""))
            .unwrap_or_default();
        zip.write_all(format!("do local _={{\n\tFamily={{\n\t\tUID=\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\n\t\tBlockType=\"EngBlock_V90_Name\",\n\t\tBlockConfig=\"EngBlock_V8_Name\"\n\t}},\n\tVariant={{\n\t\tUID=\"{car_uid}\",\n\t\tFUID=\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\",\n\t\tCapacity=2.79,\n\t\tExhaustCount=\"Exhausts_1_Name\",{aspiration}\n\t}}\n}}" ).as_bytes()).unwrap();
        zip.start_file("vehicles/test/test.pc", opt).unwrap();
        zip.write_all(format!("{{\"parts\":{{\"Camso_Engine\":\"{selected}\"}}}}").as_bytes())
            .unwrap();
        zip.start_file("vehicles/test/eng_694e8/camso_engine_694e8.jbeam", opt)
            .unwrap();
        zip.write_all(format!("{{\n\t\"{jbeam_key}\": {{}}\n}}").as_bytes())
            .unwrap();
        zip.finish().unwrap();
        path
    }

    fn fixture(car_uid: &str, selected: &str) -> PathBuf {
        fixture_with(
            car_uid,
            selected,
            Some("Aspiration_Natural_Name"),
            "Camso_Engine_694e8",
        )
    }

    #[test]
    fn matches_all_four_provenance_links_and_never_guesses_firing_order() {
        let good = fixture(UID, "Camso_Engine_694e8");
        let meta = inspect(&good, BLEND).unwrap();
        assert_eq!(meta.uid, UID);
        assert_eq!(meta.cylinders, 8);
        assert_eq!(
            meta.layout,
            Layout::V {
                bank_angle_degrees: 90
            }
        );
        assert_eq!(meta.displacement_l, Some(2.79));
        assert_eq!(meta.exhaust_count, Some(1));
        assert_eq!(meta.turbocharged, Some(false));
        assert!(inspect(&good, "art/sound/blends/other.sfxBlend2D.json").is_none());
        std::fs::remove_file(good).unwrap();

        let wrong_uid = fixture("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", "Camso_Engine_694e8");
        assert!(inspect(&wrong_uid, BLEND).is_none());
        std::fs::remove_file(wrong_uid).unwrap();
        let wrong_part = fixture(UID, "Camso_Engine_12345");
        assert!(inspect(&wrong_part, BLEND).is_none());
        std::fs::remove_file(wrong_part).unwrap();
        let wrong_jbeam = fixture_with(
            UID,
            "Camso_Engine_694e8",
            Some("Aspiration_Natural_Name"),
            "Camso_Engine_12345",
        );
        assert!(inspect(&wrong_jbeam, BLEND).is_none());
        std::fs::remove_file(wrong_jbeam).unwrap();
        let no_aspiration = fixture_with(UID, "Camso_Engine_694e8", None, "Camso_Engine_694e8");
        let meta = inspect(&no_aspiration, BLEND).unwrap();
        assert_eq!(meta.turbocharged, None);
        assert_eq!(meta.cylinders, 8);
        std::fs::remove_file(no_aspiration).unwrap();
    }

    #[test]
    fn recognizes_twelve_real_automation_variants_when_corpus_is_available() {
        let cars = Path::new(env!("CARGO_MANIFEST_DIR")).join("cars");
        if !cars.exists() {
            return;
        }
        let expected = [
            (
                "bunchyearth23_advent_tc.zip",
                8,
                Layout::V {
                    bank_angle_degrees: 90,
                },
            ),
            (
                "bunchyearth23_archo_coupe.zip",
                8,
                Layout::V {
                    bank_angle_degrees: 90,
                },
            ),
            ("bunchyearth23_b5_a.zip", 4, Layout::Inline),
            ("bunchyearth23_b5_c.zip", 4, Layout::Inline),
            ("bunchyearth23_b5_gt.zip", 4, Layout::Inline),
            ("bunchyearth23_berlingo_1000lingo.zip", 4, Layout::Inline),
            (
                "bunchyearth23_cerberus_a.zip",
                6,
                Layout::V {
                    bank_angle_degrees: 90,
                },
            ),
            ("bunchyearth23_genesis_phantom.zip", 6, Layout::Inline),
            ("bunchyearth23_nord_optis.zip", 4, Layout::Inline),
            (
                "bunchyearth23_thunderhawk_zero.zip",
                8,
                Layout::V {
                    bank_angle_degrees: 90,
                },
            ),
            ("bunchyearth23_volk_icarus.zip", 4, Layout::Inline),
            ("bunchyearth23_volk_icarus_ii.zip", 4, Layout::Inline),
        ];
        for (filename, cylinders, layout) in expected {
            let path = cars.join(filename);
            let zip = zip::ZipArchive::new(File::open(&path).unwrap()).unwrap();
            let blend = zip
                .file_names()
                .find(|name| name.ends_with(BLEND_SUFFIX))
                .unwrap()
                .to_owned();
            let meta =
                inspect(&path, &blend).unwrap_or_else(|| panic!("No metadata for {filename}"));
            assert_eq!(
                (meta.cylinders, meta.layout),
                (cylinders, layout),
                "{filename}"
            );
        }
    }
}
