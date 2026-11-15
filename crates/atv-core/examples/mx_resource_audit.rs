//! Validate decoded static collisions and textures in private MX packages.
//! Takes package paths; source resource bytes are never embedded in the tool.
use atv_core::{collision, mesh, resource::Resource, texture};

fn main() -> Result<(), String> {
    let mut counts = [0usize; 5];
    let mut failures = 0;
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let archive = Resource::parse(&bytes)?;
        counts[0] += 1;
        for (index, member) in archive.members.iter().enumerate() {
            let name = member.name.to_ascii_lowercase();
            if !name.ends_with(".pst")
                && !name.ends_with(".atm")
                && !name.ends_with(".col")
                && !name.ends_with(".psg")
            {
                continue;
            }
            let data = archive.extract(index)?;
            let result: Result<(), String> = if name.ends_with(".pst") {
                counts[1] += 1;
                texture::decode_pst(&data).map(|_| ())
            } else if name.ends_with(".atm") {
                counts[2] += 1;
                // decode_atm checks every frame extent, including unselected
                // frames; select each too to validate all pixel decodes.
                texture::decode_atm(&data, 0).and_then(|(count, _)| {
                    for frame in 1..count {
                        texture::decode_atm(&data, frame)?;
                    }
                    Ok(())
                })
            } else if name.ends_with(".psg") {
                counts[4] += 1;
                mesh::decode_psg(&data).map(|_| ())
            } else if data.get(12..16) == Some(&[0, 0, 0, 0]) {
                counts[3] += 1;
                collision::decode_mx_col(&data).map(|_| ())
            } else {
                // Type-2 movable actor rays are separate from static targets.
                continue;
            };
            if let Err(error) = result {
                eprintln!("{path}: {}: {error}", member.name);
                failures += 1;
            }
        }
    }
    if counts[0] == 0 {
        return Err("provide private MX package paths".into());
    }
    if failures != 0 {
        return Err(format!(
            "{failures} source members failed across {} packages",
            counts[0]
        ));
    }
    println!(
        "Validated {} packages, {} PSTs, {} ATMs, {} static COLs, {} PSGs",
        counts[0], counts[1], counts[2], counts[3], counts[4]
    );
    Ok(())
}
