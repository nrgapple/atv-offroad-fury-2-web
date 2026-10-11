//! Authored scene metadata. Static-model keys follow 0x002064f0/0x00207550;
//! 0x00206ba0 applies Position, LookVector and UpVector to the loaded model.
use crate::{Result, physics_data::PhysicsData};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct StaticModel {
    pub name: String,
    pub position: [f32; 3],
    pub look: [f32; 3],
    pub up: Option<[f32; 3]>,
    pub use_lighting: bool,
    pub dynamic_lighting: bool,
    /// Column-major matrix from the original up-preserving orientation path.
    pub world_matrix: [f32; 16],
}
#[derive(Debug, Serialize)]
pub struct Light {
    pub kind: String,
    pub color: [f32; 3],
    pub position: Option<[f32; 3]>,
    pub target: Option<[f32; 3]>,
    pub look: Option<[f32; 3]>,
}
#[derive(Debug, Serialize)]
pub struct CollisionModel {
    pub name: String,
    pub world_matrix: [f32; 16],
    pub material: u8,
}
#[derive(Debug, Serialize)]
pub struct Scene {
    pub start_position: [f32; 3],
    pub start_direction: [f32; 3],
    pub cube_file: String,
    pub lights: Vec<Light>,
    pub fog_color: [f32; 3],
    pub models: Vec<StaticModel>,
    pub collision_models: Vec<CollisionModel>,
}
fn value<'a>(data: &'a PhysicsData, section: &str, key: &str) -> Option<&'a str> {
    data.entries
        .iter()
        .find(|entry| entry.section == section && entry.key.eq_ignore_ascii_case(key))
        .map(|entry| entry.value.as_str())
}
fn vector(text: &str) -> Result<[f32; 3]> {
    let values = text
        .split(',')
        .map(|s| {
            s.trim()
                .parse::<f32>()
                .map_err(|_| "invalid scene vector".to_string())
        })
        .collect::<Result<Vec<_>>>()?;
    let result: [f32; 3] = values
        .try_into()
        .map_err(|_| "scene vector needs three components")?;
    if !result.iter().all(|value| value.is_finite()) {
        return Err("nonfinite scene vector".into());
    }
    Ok(result)
}
fn required_vector(data: &PhysicsData, section: &str, key: &str) -> Result<[f32; 3]> {
    vector(value(data, section, key).ok_or_else(|| format!("missing scene [{section}] {key}"))?)
}
fn optional_vector(data: &PhysicsData, section: &str, key: &str) -> Result<Option<[f32; 3]>> {
    value(data, section, key).map(vector).transpose()
}
fn count(data: &PhysicsData, section: &str, key: &str) -> Result<usize> {
    let count = value(data, section, key)
        .ok_or("missing scene count")?
        .parse::<usize>()
        .map_err(|_| "invalid scene count")?;
    if count > 4096 {
        return Err("scene count exceeds limit".into());
    }
    Ok(count)
}
fn boolean(data: &PhysicsData, section: &str, key: &str, default: bool) -> Result<bool> {
    match value(data, section, key) {
        None => Ok(default),
        Some("T" | "True" | "1") => Ok(true),
        Some("F" | "False" | "0") => Ok(false),
        Some(_) => Err(format!("unsupported scene boolean {key}")),
    }
}
fn color(data: &PhysicsData, section: &str) -> Result<[f32; 3]> {
    let color = required_vector(data, section, "ColorRGB")?;
    if color.iter().any(|v| !(0.0..=255.0).contains(v)) {
        return Err("scene color outside byte range".into());
    }
    Ok(color)
}

/// Indexed scene-parser preprocessing, 00142540: unquoted whitespace is
/// discarded outside section brackets. Thus authored `stone dock.slt` names
/// the `stonedock` asset; whitespace inside quoted values remains meaningful.
fn scene_line(line: &str) -> String {
    let mut quoted = false;
    let mut brackets = 0i32;
    line.chars()
        .take_while(|&c| c != ';')
        .filter(|&c| match c {
            '"' => {
                quoted = !quoted;
                false
            }
            '[' => {
                brackets += 1;
                true
            }
            ']' => {
                brackets -= 1;
                true
            }
            ' ' | '\t' | '\r' => quoted || brackets > 0,
            _ => true,
        })
        .collect()
}

/// 00164dd0 with orthogonalize=1, preserve-look=0. The original 0044f120
/// initializer supplies +Y up and +Z look; host sqrt differs from VU VRSQRT.
pub fn placement_matrix(position: [f32; 3], look: [f32; 3], up: [f32; 3]) -> Result<[f32; 16]> {
    fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    fn unit(value: [f32; 3]) -> Result<[f32; 3]> {
        let length = value.iter().map(|v| v * v).sum::<f32>().sqrt();
        if !length.is_finite() || length < 1e-12 {
            return Err("degenerate scene orientation".into());
        }
        Ok(value.map(|v| v / length))
    }
    if !position
        .iter()
        .chain(look.iter())
        .chain(up.iter())
        .all(|v| v.is_finite())
    {
        return Err("nonfinite scene placement".into());
    }
    let right = cross(up, look);
    let forward = unit(cross(right, up))?;
    let up = unit(up)?;
    let right = cross(up, forward);
    Ok([
        right[0],
        right[1],
        right[2],
        0.,
        up[0],
        up[1],
        up[2],
        0.,
        forward[0],
        forward[1],
        forward[2],
        0.,
        position[0],
        position[1],
        position[2],
        1.,
    ])
}
/// 00206e90 normalizes up and look separately, then crosses them. It does
/// not use the visual model's up-preserving orthogonalization.
pub fn collision_placement_matrix(
    position: [f32; 3],
    look: [f32; 3],
    up: [f32; 3],
) -> Result<[f32; 16]> {
    fn unit(v: [f32; 3]) -> Result<[f32; 3]> {
        let length = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if !length.is_finite() || length < 1e-12 {
            return Err("degenerate collision orientation".into());
        }
        Ok(v.map(|x| x / length))
    }
    if !position.iter().all(|v| v.is_finite()) {
        return Err("nonfinite collision position".into());
    }
    let look = unit(look)?;
    let up = unit(up)?;
    let right = [
        up[1] * look[2] - up[2] * look[1],
        up[2] * look[0] - up[0] * look[2],
        up[0] * look[1] - up[1] * look[0],
    ];
    Ok([
        right[0],
        right[1],
        right[2],
        0.,
        up[0],
        up[1],
        up[2],
        0.,
        look[0],
        look[1],
        look[2],
        0.,
        position[0],
        position[1],
        position[2],
        1.,
    ])
}
pub fn parse(data: &[u8]) -> Result<Scene> {
    parse_with_start(data, None)
}

fn document(data: &[u8]) -> Result<PhysicsData> {
    if data.len() > 1024 * 1024 || data.contains(&0) {
        return Err("invalid scene text extent".into());
    }
    let source = std::str::from_utf8(data).map_err(|_| "scene text is not UTF-8")?;
    let clean = source
        .lines()
        .map(scene_line)
        .collect::<Vec<_>>()
        .join("\n");
    PhysicsData::parse(clean.as_bytes())
}

/// Freestyle TOY SceneInfo supplies the mode's start, independently of the
/// SCN render/collision metadata (002fa3c0's start-position reader).
pub fn parse_with_start(data: &[u8], mode: Option<&[u8]>) -> Result<Scene> {
    let data = document(data)?;
    let mode = mode.map(document).transpose()?;
    let start = mode
        .as_ref()
        .filter(|mode| value(mode, "SceneInfo", "DefaultStartPosition").is_some())
        .unwrap_or(&data);
    let start_position = vector(
        value(start, "SceneInfo", "Start1")
            .or_else(|| value(start, "SceneInfo", "DefaultStartPosition"))
            .ok_or("missing scene start")?,
    )?;
    let start_direction = vector(
        value(start, "SceneInfo", "Direction1")
            .or_else(|| value(start, "SceneInfo", "DefaultStartDirection"))
            .ok_or("missing scene start direction")?,
    )?;
    let cube_file = value(&data, "Environment", "CubeFile")
        .ok_or("missing scene cube file")?
        .to_owned();
    let mut models = Vec::new();
    for index in 1..=count(&data, "StaticModels", "NumberOfStaticModels")? {
        let section = format!("Model{index}");
        if value(&data, &section, "StaticType").is_some_and(|value| value != "Standard") {
            return Err("unsupported scene static model type".into());
        }
        // 002064f0 truncates SLT at its first dot; 001b2d40 forces .psg.
        let name = value(&data, &section, "SLT")
            .ok_or("missing scene model name")?
            .split('.')
            .next()
            .unwrap_or("");
        if name.is_empty() || name.len() > 50 || name.contains(['/', '\\', ':']) {
            return Err("unsupported scene model name".into());
        }
        let position = required_vector(&data, &section, "Position")?;
        let look = required_vector(&data, &section, "LookVector")?;
        let up = optional_vector(&data, &section, "UpVector")?;
        models.push(StaticModel {
            name: name.to_owned(),
            position,
            look,
            up,
            use_lighting: boolean(&data, &section, "UseLighting", true)?,
            dynamic_lighting: boolean(&data, &section, "UseDynamicLighting", false)?,
            world_matrix: placement_matrix(position, look, up.unwrap_or([0., 1., 0.]))?,
        });
    }
    let mut lights = Vec::new();
    for index in 1..=count(&data, "Lights", "NumberOfLights")? {
        let section = format!("Light{index}");
        lights.push(Light {
            kind: value(&data, &section, "Type")
                .ok_or("missing scene light type")?
                .to_owned(),
            color: color(&data, &section)?,
            position: optional_vector(&data, &section, "Position")?,
            target: optional_vector(&data, &section, "TargetPosition")?,
            look: optional_vector(&data, &section, "LookVector")?,
        });
    }
    // 00201f60/00207a10: absent/-1 collision count reuses Model%d in
    // authored order. An explicit override uses Object%d instead.
    let override_count = value(&data, "CollisionObjects", "NumberOfCollisionObjects")
        .map(|text| text.parse::<i32>().map_err(|_| "invalid collision count"))
        .transpose()?;
    let override_count = match override_count {
        None | Some(-1) => None,
        Some(n) if (0..=4096).contains(&n) => Some(n as usize),
        _ => return Err("collision count exceeds limit".into()),
    };
    let mut collision_models = Vec::new();
    for index in 1..=override_count.unwrap_or(models.len()) {
        let section = format!(
            "{}{index}",
            if override_count.is_some() {
                "Object"
            } else {
                "Model"
            }
        );
        if value(&data, &section, "StaticType").is_some_and(|value| value != "Standard") {
            return Err("unsupported scene collision type".into());
        }
        let name = value(&data, &section, "COL")
            .or_else(|| value(&data, &section, "SLT"))
            .ok_or("missing scene collision name")?
            .split('.')
            .next()
            .unwrap_or("");
        if name.is_empty() {
            continue;
        }
        if name.len() > 50 || name.contains(['/', '\\', ':']) {
            return Err("unsupported scene collision name".into());
        }
        collision_models.push(CollisionModel {
            name: name.to_owned(),
            world_matrix: collision_placement_matrix(
                optional_vector(&data, &section, "Position")?.unwrap_or([0.; 3]),
                optional_vector(&data, &section, "LookVector")?.unwrap_or([0., 0., 1.]),
                optional_vector(&data, &section, "UpVector")?.unwrap_or([0., 1., 0.]),
            )?,
            // 00206900 initializes the collision material to zero;
            // the Standard reader00206940 does not override it.
            material: 0,
        });
    }
    Ok(Scene {
        start_position,
        start_direction,
        cube_file,
        lights,
        fog_color: color(&data, "Fog")?,
        models,
        collision_models,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_scene_whitespace_rules_resolve_visual_and_collision_names() {
        for (authored, expected) in [
            ("stone dock.slt", "stonedock"),
            ("bike4\t.slt", "bike4"),
            ("\"two words.slt\"", "two words"),
        ] {
            let scene = parse(SOURCE.replace("sample.slt", authored).as_bytes()).unwrap();
            assert_eq!(scene.models[0].name, expected);
            assert_eq!(scene.collision_models[0].name, expected);
        }
        assert_eq!(scene_line("[A section]"), "[A section]");
    }
    const SOURCE: &str = "[SceneInfo]\nDefaultStartPosition=1,2,3\nDefaultStartDirection=0,0,1\nStart1=4,5,6 ; authored override\n[Environment]\nCubeFile=sample.cub\n[Lights]\nNumberOfLights=1\n[Light1]\nType=Ambient\nColorRGB=20,30,40\n[Fog]\nColorRGB=80,90,100\n[StaticModels]\nNumberOfStaticModels=1\n[Model1]\nSLT=sample.slt\nPosition=3,4,5\nLookVector=0,0,-1\n";
    #[test]
    fn authored_overrides_names_and_flags() {
        let scene = parse(SOURCE.as_bytes()).unwrap();
        assert_eq!(scene.start_position, [4., 5., 6.]);
        assert_eq!(scene.models[0].name, "sample");
        assert!(scene.models[0].use_lighting);
        assert!(!scene.models[0].dynamic_lighting);
        assert!(scene.models[0].up.is_none());
        assert_eq!(scene.lights[0].color, [20., 30., 40.]);
        assert_eq!(scene.collision_models[0].name, "sample");
        assert_eq!(scene.collision_models[0].material, 0);
    }
    #[test]
    fn freestyle_start_is_separate_from_render_scene() {
        let source = SOURCE.replace("DefaultStartDirection=0,0,1\n", "");
        let mode = b"[SceneInfo]\nDefaultStartPosition=7,8,9\nDefaultStartDirection=1,0,0\n";
        assert!(parse(source.as_bytes()).is_err());
        let scene = parse_with_start(source.as_bytes(), Some(mode)).unwrap();
        assert_eq!(scene.start_position, [7., 8., 9.]);
        assert_eq!(scene.start_direction, [1., 0., 0.]);
        assert_eq!(scene.models[0].name, "sample");
        assert_eq!(
            parse_with_start(
                SOURCE.as_bytes(),
                Some(b"[Animations]\nNumberOfAnimations=0")
            )
            .unwrap()
            .start_position,
            [4., 5., 6.]
        );
        assert!(
            parse_with_start(
                SOURCE.as_bytes(),
                Some(b"[SceneInfo]\nDefaultStartPosition=NaN,0,0")
            )
            .is_err()
        );
    }
    #[test]
    fn invalid_extents_vectors_and_models_fail() {
        for invalid in [
            SOURCE.replace("3,4,5", "NaN,4,5"),
            SOURCE.replace("NumberOfStaticModels=1", "NumberOfStaticModels=99999"),
            SOURCE.replace("sample.slt", "../sample.slt"),
            SOURCE.replace("0,0,-1", "0,1"),
            SOURCE.replace("80,90,100", "256,0,0"),
        ] {
            assert!(parse(invalid.as_bytes()).is_err());
        }
    }
    #[test]
    fn orientation_preserves_up_and_orthogonalizes_look() {
        let matrix = placement_matrix([2., 3., 4.], [2., 2., 0.], [0., 3., 0.]).unwrap();
        assert_eq!(
            matrix,
            [
                0., 0., -1., 0., 0., 1., 0., 0., 1., 0., 0., 0., 2., 3., 4., 1.
            ]
        );
        assert!(placement_matrix([0.; 3], [0., 1., 0.], [0., 1., 0.]).is_err());
    }
    #[test]
    fn collision_override_order_and_separate_orientation() {
        let source = format!(
            "{SOURCE}\n[CollisionObjects]\nNumberOfCollisionObjects=2\n[Object1]\nCOL=first.col\nSLT=unused.psg\nPosition=1,2,3\nLookVector=0,2,2\nUpVector=0,3,0\n[Object2]\nSLT=second.psg\n"
        );
        let scene = parse(source.as_bytes()).unwrap();
        assert_eq!(
            scene
                .collision_models
                .iter()
                .map(|m| m.name.as_str())
                .collect::<Vec<_>>(),
            ["first", "second"]
        );
        let first = &scene.collision_models[0].world_matrix;
        assert_eq!(&first[12..15], &[1., 2., 3.]);
        assert!((first[9] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert_eq!(&first[4..7], &[0., 1., 0.]);
        assert_ne!(
            first,
            &placement_matrix([1., 2., 3.], [0., 2., 2.], [0., 3., 0.]).unwrap()
        );
        assert!(
            parse(
                format!("{SOURCE}\n[CollisionObjects]\nNumberOfCollisionObjects=-2\n").as_bytes()
            )
            .is_err()
        );
        assert_eq!(
            parse(
                format!("{SOURCE}\n[CollisionObjects]\nNumberOfCollisionObjects=-1\n").as_bytes()
            )
            .unwrap()
            .collision_models[0]
                .name,
            "sample"
        );
    }
}
