use super::Cube;

fn triple<'a>(mut words: impl Iterator<Item = &'a str>, line: usize) -> Result<[f32; 3], String> {
    let mut out = [0.0; 3];
    for slot in &mut out {
        *slot = words
            .next()
            .and_then(|w| w.parse().ok())
            .ok_or_else(|| format!("line {line}: expected three numbers"))?;
    }
    Ok(out)
}

pub fn parse_cube(text: &str) -> Result<Cube, String> {
    let mut cube = Cube {
        size: 0,
        domain_min: [0.0; 3],
        domain_max: [1.0; 3],
        data: Vec::new(),
    };
    for (i, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        match words.next().unwrap_or("") {
            "LUT_3D_SIZE" => {
                cube.size = words
                    .next()
                    .and_then(|w| w.parse().ok())
                    .filter(|n| (2..=256).contains(n))
                    .ok_or_else(|| format!("line {}: LUT_3D_SIZE must be 2..=256", i + 1))?;
            }
            "LUT_1D_SIZE" => return Err("1D LUTs are not supported, only LUT_3D_SIZE".into()),
            "DOMAIN_MIN" => cube.domain_min = triple(words, i + 1)?,
            "DOMAIN_MAX" => cube.domain_max = triple(words, i + 1)?,
            word if word.starts_with(|c: char| c.is_ascii_alphabetic()) => {}
            _ => cube.data.push(triple(line.split_whitespace(), i + 1)?),
        }
    }
    let expected = (cube.size as usize).pow(3);
    if cube.size == 0 {
        return Err("missing LUT_3D_SIZE".into());
    }
    if cube.data.len() != expected {
        return Err(format!(
            "LUT_3D_SIZE {} needs {expected} rows, found {}",
            cube.size,
            cube.data.len()
        ));
    }
    Ok(cube)
}

pub fn load_cube(path: &str) -> Result<Cube, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    parse_cube(&text).map_err(|e| format!("{path}: {e}"))
}
