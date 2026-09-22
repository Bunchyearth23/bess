use std::{fs::File, io::Read, path::Path};

#[derive(Debug)]
pub struct Vehicle {
    pub name: String,
    pub audio_files: usize,
    pub blend_files: usize,
    pub idle_rpm: Option<f32>,
    pub max_rpm: Option<f32>,
    pub cylinders: Option<u32>,
    pub engine_files: usize,
}
// JBeam permits omitted commas. Lex quoted keys instead of treating it as JSON5.
// This intentionally inspects literals only; it does not evaluate JBeam parts.
fn numbers(text: &str, key: &str, out: &mut Vec<f32>) {
    let bytes = text.as_bytes();
    let mut i = 0;
    let mut matched = false;
    let mut colon = false;
    while i < bytes.len() {
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        if bytes[i..].starts_with(b"//") {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if bytes[i..].starts_with(b"/*") {
            i += 2;
            while i < bytes.len() && !bytes[i..].starts_with(b"*/") {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }
        if bytes[i] == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'\\' {
                    i = (i + 2).min(bytes.len());
                } else if bytes[i] == b'"' {
                    i += 1;
                    break;
                } else {
                    i += 1;
                }
            }
            matched = serde_json::from_str::<String>(&text[start..i]).is_ok_and(|s| s == key);
            colon = false;
            continue;
        }
        if bytes[i] == b':' && matched {
            colon = true;
            i += 1;
            continue;
        }
        if colon {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || b"+-.eE".contains(&bytes[i])) {
                i += 1;
            }
            let boundary =
                i == bytes.len() || bytes[i].is_ascii_whitespace() || b",}]/".contains(&bytes[i]);
            if boundary
                && let Ok(n) = text[start..i].parse::<f32>()
                && n.is_finite()
                && !out.contains(&n)
            {
                out.push(n);
            }
            if i == start {
                i += 1;
            }
        } else {
            i += 1;
        }
        matched = false;
        colon = false;
    }
}
fn unique(values: &[f32]) -> Option<f32> {
    if values.len() == 1 {
        Some(values[0])
    } else {
        None
    }
}

/// Inspect bounded metadata in-place. Never extracts files or modifies the source ZIP.
pub fn inspect(path: &Path) -> Result<Vehicle, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > 20000 {
        return Err("Too many files in the archive".into());
    }
    let mut names = Vec::new();
    let mut idle = Vec::new();
    let mut max = Vec::new();
    let mut cylinders = Vec::new();
    let mut result = Vehicle {
        name: String::new(),
        audio_files: 0,
        blend_files: 0,
        idle_rpm: None,
        max_rpm: None,
        cylinders: None,
        engine_files: 0,
    };
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().to_owned();
        if name.starts_with("vehicles/")
            && let Some(vehicle) = name.split('/').nth(1)
            && !names.contains(&vehicle.to_owned())
        {
            names.push(vehicle.to_owned());
        }
        if name.starts_with("art/sound/") && name.ends_with(".wav") {
            result.audio_files += 1;
        }
        if name.ends_with(".sfxBlend2D.json") {
            result.blend_files += 1;
        }
        let basename = name.rsplit('/').next().unwrap_or("");
        if name.contains("/eng_")
            && basename.starts_with("camso_engine_")
            && basename.ends_with(".jbeam")
        {
            if entry.size() > 4_000_000 {
                return Err(format!("Metadata file too large: {name}"));
            }
            let mut text = String::new();
            entry
                .take(4_000_001)
                .read_to_string(&mut text)
                .map_err(|e| e.to_string())?;
            if text.len() > 4_000_000 {
                return Err("Read limit exceeded".into());
            }
            numbers(&text, "idleRPM", &mut idle);
            numbers(&text, "maxRPM", &mut max);
            numbers(&text, "fundamentalFrequencyCylinderCount", &mut cylinders);
            result.engine_files += 1;
        }
    }
    if names.len() != 1 || result.engine_files == 0 {
        return Err("Select an Automation vehicle containing a camso_engine_*.jbeam file".into());
    }
    result.name = names.remove(0);
    result.idle_rpm = unique(&idle);
    result.max_rpm = unique(&max);
    result.cylinders = unique(&cylinders)
        .filter(|n| *n >= 1. && *n <= 12. && n.fract() == 0.)
        .map(|n| n as u32);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comments_are_not_engine_metadata() {
        let v = "{ // \"fundamentalFrequencyCylinderCount\": 4,\n \"mainEngine\": {\"idleRPM\":800 \"maxRPM\":12000} }";
        let mut c = Vec::new();
        numbers(v, "fundamentalFrequencyCylinderCount", &mut c);
        assert!(c.is_empty());
        numbers(v, "idleRPM", &mut c);
        assert_eq!(unique(&c), Some(800.));
    }
    #[test]
    fn strings_block_comments_and_expressions_are_ignored() {
        let text = r#"{"note":"\"idleRPM\":999", /* "idleRPM": 500 */ "idleRPM":"$idleRPM" "maxRPM":12000}"#;
        let mut values = Vec::new();
        numbers(text, "idleRPM", &mut values);
        assert!(values.is_empty());
        numbers(text, "maxRPM", &mut values);
        assert_eq!(values, vec![12000.]);
    }
}
