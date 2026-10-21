//! RGBA8 material textures with the box-filtered mip chain WebGL's
//! generateMipmap produces, and the browser's sampler states.
use crate::Gpu;

/// Layer count for a texture the GL backend must create as TEXTURE_2D_ARRAY:
/// it infers a cube (array) from a multiple of six layers, which WebGL then
/// rejects for 2D-array views (ENV04 has 102 terrain layers). One unused
/// layer avoids the guess. Single-layer textures stay 2D.
pub fn array_layers(layers: u32) -> u32 {
    if layers > 1 && layers.is_multiple_of(6) {
        layers + 1
    } else {
        layers
    }
}

pub fn mip_levels(width: u32, height: u32) -> u32 {
    32 - width.max(height).leading_zeros()
}

/// Uploads one RGBA8 image with a full mip chain.
pub fn upload(gpu: &Gpu, width: u32, height: u32, rgba: &[u8], label: &str) -> wgpu::Texture {
    let levels = mip_levels(width, height);
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    write(gpu, &texture, width, height, rgba);
    texture
}

/// Uploads one RGBA8 image without mips (LINEAR, as particle atlases are).
pub fn upload_flat(gpu: &Gpu, width: u32, height: u32, rgba: &[u8], label: &str) -> wgpu::Texture {
    use wgpu::util::DeviceExt;
    gpu.device.create_texture_with_data(
        &gpu.queue,
        &wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        },
        wgpu::util::TextureDataOrder::LayerMajor,
        rgba,
    )
}

/// Rewrites every level of a texture made by [`upload`] from new pixels.
pub fn write(gpu: &Gpu, texture: &wgpu::Texture, width: u32, height: u32, rgba: &[u8]) {
    let levels = texture.mip_level_count();
    let (mut w, mut h) = (width, height);
    let mut pixels = std::borrow::Cow::Borrowed(rgba);
    for level in 0..levels {
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        if level + 1 < levels {
            let (next, nw, nh) = downsample(&pixels, w, h);
            (pixels, w, h) = (std::borrow::Cow::Owned(next), nw, nh);
        }
    }
}

/// Writes every level of one layer of an array texture (box-filtered mips as [`write`]).
pub fn write_layer(
    gpu: &Gpu,
    texture: &wgpu::Texture,
    layer: u32,
    width: u32,
    height: u32,
    rgba: &[u8],
) {
    let levels = texture.mip_level_count();
    let (mut w, mut h) = (width, height);
    let mut pixels = std::borrow::Cow::Borrowed(rgba);
    for level in 0..levels {
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: layer,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w * 4),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        if level + 1 < levels {
            let (next, nw, nh) = downsample(&pixels, w, h);
            (pixels, w, h) = (std::borrow::Cow::Owned(next), nw, nh);
        }
    }
}

pub fn downsample(pixels: &[u8], w: u32, h: u32) -> (Vec<u8>, u32, u32) {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = vec![0u8; (nw * nh * 4) as usize];
    for y in 0..nh {
        for x in 0..nw {
            for c in 0..4 {
                let mut sum = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let (sx, sy) = ((x * 2 + dx).min(w - 1), (y * 2 + dy).min(h - 1));
                    sum += pixels[((sy * w + sx) * 4 + c) as usize] as u32;
                }
                out[((y * nw + x) * 4 + c) as usize] = ((sum + 2) / 4) as u8;
            }
        }
    }
    (out, nw, nh)
}

/// LINEAR_MIPMAP_LINEAR with REPEAT (material textures) or CLAMP_TO_EDGE (sky).
pub fn sampler(gpu: &Gpu, repeat: bool, anisotropy: u16) -> wgpu::Sampler {
    let mode = if repeat {
        wgpu::AddressMode::Repeat
    } else {
        wgpu::AddressMode::ClampToEdge
    };
    gpu.device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some(if repeat { "material" } else { "sky" }),
        address_mode_u: mode,
        address_mode_v: mode,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Linear,
        anisotropy_clamp: anisotropy.max(1),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn array_layers_never_look_like_cube_maps() {
        assert_eq!(super::array_layers(1), 1);
        assert_eq!(super::array_layers(6), 7);
        assert_eq!(super::array_layers(102), 103);
        assert_eq!(super::array_layers(34), 34);
    }
}
