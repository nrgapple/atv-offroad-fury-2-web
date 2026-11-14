//! Local source validation: filenames are private CUB43 resources; no source
//! content is embedded in this example or its output.
fn main() -> Result<(), String> {
    let mut skies = 0;
    let mut tiles = 0;
    for path in std::env::args().skip(1) {
        let bytes = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
        let sky = atv_core::mx_sky::decode(&bytes).map_err(|e| format!("{path}: {e}"))?;
        for (index, tile) in sky.tiles.iter().enumerate() {
            tile.rgba()
                .map_err(|e| format!("{path}, tile {index}: {e}"))?;
            tiles += 1;
        }
        skies += 1;
    }
    if skies == 0 {
        return Err("provide private CUB43 paths".into());
    }
    println!("Validated {skies} skies and {tiles} fully covered tiles");
    Ok(())
}
