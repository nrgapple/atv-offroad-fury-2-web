//! Opt-in geometry check using private original assets and a browser trace.
//! Run after test_clean_landing.mjs; no original content is embedded in this tool.
use atv_core::{Result, dynamics, mesh, obstacle::transform_point, terrain};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
struct Sample {
    step: usize,
    frame: Frame,
}
#[derive(Deserialize)]
struct Frame {
    origin: [f32; 3],
    orientation: [f32; 4],
    quad_node_matrices: Vec<[f32; 16]>,
    wreck: Wreck,
}
#[derive(Deserialize)]
struct Wreck {
    phase: String,
}

fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if !(3..=4).contains(&args.len()) {
        return Err(
            "usage: terrain_clearance <quad.psg> <terrain.tr2> <trace.json> [maximum-overlap-feet]"
                .into(),
        );
    }
    let maximum_overlap: Option<f32> = args
        .get(3)
        .map(|s| s.parse().map_err(|_| "invalid overlap limit".to_owned()))
        .transpose()?;
    if maximum_overlap.is_some_and(|v| !v.is_finite() || v < 0.) {
        return Err("overlap limit must be finite and nonnegative".into());
    }
    let read = |path: &str| std::fs::read(path).map_err(|e| format!("{path}: {e}"));
    let mesh = mesh::decode_psg(&read(&args[0])?)?;
    let terrain = terrain::decode_tr2(&read(&args[1])?)?;
    let trace: Vec<Sample> = serde_json::from_slice(&read(&args[2])?).map_err(|e| e.to_string())?;
    let names = [
        "WheelFrontLeft",
        "WheelFrontRight",
        "WheelRearLeft",
        "WheelRearRight",
    ];
    let wheels: Vec<_> = names
        .iter()
        .map(|name| {
            mesh.nodes
                .iter()
                .position(|n| n.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("missing {name}"))
        })
        .collect::<Result<_>>()?;
    let lod = mesh.lods.first().ok_or("missing visible quad geometry")?;
    let vertices: Vec<Vec<_>> = wheels
        .iter()
        .map(|&wheel| {
            lod.groups
                .iter()
                .flat_map(|g| &g.vertices)
                .filter(|v| usize::from(v.node) == wheel)
                .map(|v| v.local_position)
                .collect()
        })
        .collect();
    if vertices.iter().any(Vec::is_empty) {
        return Err("missing visible wheel vertices".into());
    }
    let mut measured = 0;
    let mut worst = f32::INFINITY;
    let mut worst_step = 0;
    let mut worst_wheel = 0;
    let mut minima = [f32::INFINITY; 4];
    for sample in &trace {
        let frame = &sample.frame;
        if frame.wreck.phase != "Riding" {
            continue;
        }
        if frame.quad_node_matrices.len() != mesh.nodes.len()
            || !frame
                .origin
                .iter()
                .chain(&frame.orientation)
                .chain(frame.quad_node_matrices.iter().flatten())
                .all(|v| v.is_finite())
        {
            return Err(format!(
                "invalid rendered transform at step {}",
                sample.step
            ));
        }
        let b = dynamics::basis(frame.orientation);
        let p = frame.origin;
        let world = [
            b[0][0], b[0][1], b[0][2], 0., b[1][0], b[1][1], b[1][2], 0., b[2][0], b[2][1],
            b[2][2], 0., p[0], p[1], p[2], 1.,
        ];
        for (wheel, (&node, positions)) in wheels.iter().zip(&vertices).enumerate() {
            for &vertex in positions {
                let p = transform_point(
                    &world,
                    transform_point(&frame.quad_node_matrices[node], vertex),
                );
                let ground = terrain
                    .contact(p[0], p[2])?
                    .ok_or_else(|| format!("wheel left terrain at step {}", sample.step))?;
                let clearance = p[1] - ground.height;
                minima[wheel] = minima[wheel].min(clearance);
                if clearance < worst {
                    worst = clearance;
                    worst_step = sample.step;
                    worst_wheel = wheel;
                }
            }
        }
        measured += 1;
    }
    if measured == 0 {
        return Err("trace contains no riding frames".into());
    }
    println!(
        "{}",
        json!({
            "riding_frames": measured,
            "skipped_frames": trace.len() - measured,
            "minimum_clearance_feet": worst,
            "per_wheel_minimum_feet": minima,
            "worst_step": worst_step,
            "worst_wheel": names[worst_wheel],
            "maximum_overlap_feet": maximum_overlap,
            "scope": "Recorded rendered tire vertices against full terrain; excludes wreck frames and does not establish original-game parity"
        })
    );
    if maximum_overlap.is_some_and(|limit| worst < -limit) {
        return Err(format!(
            "tire overlap {} feet exceeds the requested limit",
            -worst
        ));
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
