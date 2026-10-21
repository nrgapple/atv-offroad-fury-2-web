//! Loads one venue's terrain presentation from its original course archive.
//! Mirrors terrainFromContent/uploadTerrainTextures in web/game.js.
use crate::{Result, launch::course_resource};
use atv_content::Content;
use atv_core::{ipu, resource::Resource, scene::Scene, terrain, texture};

pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub struct Terrain {
    /// Vertex counts along X and Z (cells + 1).
    pub width: usize,
    pub depth: usize,
    pub spacing: f32,
    /// Row-major Z then X.
    pub heights: Vec<f32>,
    pub min: f32,
    pub max: f32,
    /// 256x256 base layers, one per original texture record.
    pub layers: Vec<Image>,
    /// Layer per 64-cell tile, row-major, `mapping_width` x `mapping_height`.
    pub mapping: Vec<u16>,
    pub mapping_width: u32,
    pub mapping_height: u32,
    pub detail: Image,
}

impl Terrain {
    pub fn size_x(&self) -> f32 {
        (self.width - 1) as f32 * self.spacing
    }
    pub fn size_z(&self) -> f32 {
        (self.depth - 1) as f32 * self.spacing
    }
    pub fn height(&self, x: usize, z: usize) -> f32 {
        self.heights[z * self.width + x]
    }
}

pub struct Venue {
    pub environment: String,
    pub scene: Scene,
    pub terrain: Terrain,
}

fn member_named(resource: &Resource, name: &str) -> Option<usize> {
    resource.members.iter().position(|member| {
        member
            .name
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|last| last.eq_ignore_ascii_case(name))
    })
}

pub async fn load(content: &Content, environment: &str, racing: bool) -> Result<Venue> {
    let course = course_resource(environment, racing)?;
    let bytes = content.file(&course.archive).await?;
    let archive = Resource::parse(&bytes)?;
    let extract = |name: &str, what: &str| -> Result<Vec<u8>> {
        archive.extract(
            member_named(&archive, name)
                .ok_or_else(|| format!("original {what} is missing ({name})"))?,
        )
    };
    let scene_text = extract(&format!("{}.scn", course.scene_stem), "start position")?;
    let mode = member_named(&archive, &format!("{}.toy", course.mode_stem))
        .map(|i| archive.extract(i))
        .transpose()?;
    let scene = atv_core::scene::parse_with_start(&scene_text, mode.as_deref())?;
    let tr2 = extract(&format!("{environment}.tr2"), "terrain")?;
    let decoded = terrain::decode_tr2(&tr2)?;
    let heights = decoded.heightfield()?;
    let (width, depth) = (
        decoded.width_cells as usize + 1,
        decoded.depth_cells as usize + 1,
    );
    if width < 2
        || depth < 2
        || !(decoded.spacing.is_finite() && decoded.spacing > 0.)
        || heights.len() != width * depth
    {
        return Err("invalid terrain dimensions".into());
    }
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for &h in &heights {
        if !h.is_finite() {
            return Err("nonfinite terrain height".into());
        }
        min = min.min(h);
        max = max.max(h);
    }

    let layout = terrain::texture_layout(&tr2)?;
    let mut layers = Vec::with_capacity(layout.records.len());
    for record in &layout.records {
        let mip = record.mips.last().ok_or("terrain texture has no mip")?;
        let image = ipu::decode_ipum(
            tr2.get(mip.offset..mip.offset + mip.bytes)
                .ok_or("terrain texture exceeds archive")?,
        )?;
        if record.width != 256 || record.height != 256 || image.rgba.len() != 256 * 256 * 4 {
            return Err("unsupported terrain texture image".into());
        }
        layers.push(Image {
            width: 256,
            height: 256,
            rgba: image.rgba,
        });
    }
    let mapping_width = (width - 1).div_ceil(64) as u32;
    let mapping_height = (depth - 1).div_ceil(64) as u32;
    let mut mapping = vec![u16::MAX; (mapping_width * mapping_height) as usize];
    for region in &layout.regions {
        if region.span_cells != 256 || region.x % 256 != 0 || region.z % 256 != 0 {
            return Err("unsupported terrain texture region".into());
        }
        for (index, layer) in region.subtextures.iter().enumerate() {
            let layer = layer.ok_or("unsupported terrain texture region")?;
            let x = region.x / 64 + index as u32 % 4;
            let z = region.z / 64 + index as u32 / 4;
            let slot = (z * mapping_width + x) as usize;
            if x >= mapping_width
                || z >= mapping_height
                || layer as usize >= layers.len()
                || mapping[slot] != u16::MAX
            {
                return Err("invalid terrain texture region coverage".into());
            }
            mapping[slot] = layer;
        }
    }
    if mapping.contains(&u16::MAX) {
        return Err("terrain texture regions do not cover the terrain".into());
    }
    // 002098b0 derives the original detail resource from the terrain basename.
    let detail = texture::decode_pst(&extract(
        &format!("{environment}_d.pst"),
        "ground detail texture",
    )?)?;
    Ok(Venue {
        environment: environment.to_string(),
        scene,
        terrain: Terrain {
            width,
            depth,
            spacing: decoded.spacing,
            heights,
            min,
            max,
            layers,
            mapping,
            mapping_width,
            mapping_height,
            detail: Image {
                width: detail.info.width,
                height: detail.info.height,
                rgba: detail.rgba,
            },
        },
    })
}
