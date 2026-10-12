//! USA v2.00 ESB proxy records, constructor geometry and placement state.
//! Record kinds differ from runtime actor kinds. Contact dispatch/response and
//! generated acceleration trees are separate, source-verified consumer work.
use crate::{Result, ecosystem::Species, vegetation_collision, wheel::Vec3};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    ModelMesh,
    Cylinder,
    Sphere,
    Box,
}
#[derive(Debug, Clone, Copy)]
pub struct Cylinder {
    pub center: Vec3,
    pub radius: f32,
    pub height: f32,
}
#[derive(Debug, Clone, Copy)]
pub struct Sphere {
    pub center: Vec3,
    pub radius: f32,
}
#[derive(Debug, Clone, Copy)]
pub struct BoxShape {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy)]
pub enum Record {
    /// 001286f0 delegates to the species model's COL resource. The record's
    ///other words do not configure that mesh and must not be interpreted.
    ModelMesh,
    Cylinder(Cylinder),
    Sphere(Sphere),
    Box(BoxShape),
}
impl Record {
    /// 00128ca0 expands nine file words into a twelve-word runtime descriptor:
    ///kind, centerXYZ, halfXYZ, radius, height; the two vector W values become 1.
    /// Only fields consumed by the selected constructor are validated. Shipped
    ///cylinders have unused half words containing arbitrary bits, including NaNs.
    pub fn parse(words: &[u32; 9]) -> Result<Self> {
        let center = || [words[1], words[2], words[3]].map(f32::from_bits);
        let record = match words[0] {
            0 => Self::ModelMesh,
            1 => Self::Cylinder(Cylinder {
                center: center(),
                radius: f32::from_bits(words[7]),
                height: f32::from_bits(words[8]),
            }),
            2 => Self::Sphere(Sphere {
                center: center(),
                radius: f32::from_bits(words[7]),
            }),
            3 => Self::Box(BoxShape {
                center: center(),
                half_extents: [words[4], words[5], words[6]].map(f32::from_bits),
                radius: f32::from_bits(words[7]),
            }),
            _ => return Err("unsupported vegetation proxy record kind".into()),
        };
        let finite = match record {
            Self::ModelMesh => true,
            Self::Cylinder(c) => c
                .center
                .iter()
                .copied()
                .chain([c.radius, c.height])
                .all(f32::is_finite),
            Self::Sphere(s) => s
                .center
                .iter()
                .copied()
                .chain([s.radius])
                .all(f32::is_finite),
            Self::Box(b) => b
                .center
                .iter()
                .chain(b.half_extents.iter())
                .copied()
                .chain([b.radius])
                .all(f32::is_finite),
        };
        if !finite {
            return Err("nonfinite used vegetation proxy field".into());
        }
        // Native does not reject signed or zero dimensions. In particular, one
        // shipped cylinder has zero radius/height and still occupies its slot.
        Ok(record)
    }
    pub fn kind(self) -> Kind {
        match self {
            Self::ModelMesh => Kind::ModelMesh,
            Self::Cylinder(_) => Kind::Cylinder,
            Self::Sphere(_) => Kind::Sphere,
            Self::Box(_) => Kind::Box,
        }
    }
    /// Constructor runtime tags, not the ESB kind word or category mask.
    pub fn runtime_actor_kind(self) -> u32 {
        match self {
            Self::ModelMesh | Self::Cylinder(_) => 0,
            Self::Sphere(_) => 5,
            Self::Box(_) => 4,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CylinderMesh {
    ///Top ring first, then bottom ring, with the authored local center added.
    pub vertices: [Vec3; 32],
    ///Two outward side triangles per sector. The original cylinder has no caps.
    pub triangles: [[u16; 3]; 32],
}
impl Cylinder {
    /// 001286f0 emits this input into 0032eb70, before native tree construction.
    /// Zero dimensions preserve the same 32 vertices/faces; consumers must retain
    ///source slot/order and the native handling of degenerate face normals.
    pub fn mesh(self) -> Result<CylinderMesh> {
        if !self
            .center
            .iter()
            .copied()
            .chain([self.radius, self.height])
            .all(f32::is_finite)
        {
            return Err("nonfinite cylinder construction field".into());
        }
        let mut vertices = [[0.; 3]; 32];
        for sector in 0..16 {
            let mut angle = sector as f32 * f32::from_bits(0x3ec90fdb);
            if angle > f32::from_bits(0x40490fdb) {
                angle -= f32::from_bits(0x40c90fdb);
            } else if angle < -f32::from_bits(0x40490fdb) {
                angle += f32::from_bits(0x40c90fdb);
            }
            let (sine, cosine) = constructor_sine_cosine(angle);
            for (index, y) in [(sector, self.height), (sector + 16, 0.)] {
                vertices[index] = [
                    cosine * self.radius + self.center[0],
                    y + self.center[1],
                    sine * self.radius + self.center[2],
                ];
            }
        }
        let triangles = std::array::from_fn(|face| {
            let sector = (face / 2) as u16;
            let next = (sector + 1) & 15;
            if face & 1 == 0 {
                [sector, next, next + 16]
            } else {
                [sector, next + 16, sector + 16]
            }
        });
        if vertices.iter().flatten().any(|value| !value.is_finite()) {
            return Err("nonfinite cylinder construction result".into());
        }
        Ok(CylinderMesh {
            vertices,
            triangles,
        })
    }
}

/// 001ab3c0 uses these binary32 polynomials after the constructor wraps to ±pi.
/// Host sin/cos would change the generated vertex and subsequent plane bits.
fn constructor_sine_cosine(angle: f32) -> (f32, f32) {
    let square = angle * angle;
    let sine = [
        0xbe2aaaab, 0x3c088889, 0xb9500d01, 0x3638ef1d, 0xb2d7322b, 0x2f309231, 0xab573fa0,
    ]
    .map(f32::from_bits);
    let cosine = [
        0xbf000000, 0x3d2aaaab, 0xbab60b61, 0x37d00d01, 0xb493f27e, 0x310f76c7, 0xad49cba6,
        0x29573fa0,
    ]
    .map(f32::from_bits);
    let mut s = sine[6];
    for &coefficient in sine[..6].iter().rev() {
        s = coefficient + square * s;
    }
    let mut c = cosine[7];
    for &coefficient in cosine[..7].iter().rev() {
        c = coefficient + square * c;
    }
    (angle * (square * s + 1.), square * c + 1.)
}

#[derive(Debug, Clone, Copy)]
pub struct PlacementTransform {
    ///Final packed XYZ converted with the original extent/65536 unit. Never
    ///resample again here: registration already quantized the authored Y.
    pub position: Vec3,
    ///Stored for runtime sphere/box actors; runtime kind0 does not apply either.
    pub width_ratio: f32,
    pub height_ratio: f32,
}
impl PlacementTransform {
    /// 001280b0 divides the original signed-byte dimension outputs by species
    ///base width/height. Passing already decoded placement dimensions preserves
    ///the same operations without retaining a second copy of raw ESB bytes.
    pub fn from_dimensions(
        species: &Species,
        coordinates: [u16; 3],
        coordinate_unit: f32,
        height: f32,
        width: f32,
    ) -> Result<Self> {
        let base_height = f32::from_bits(species.parameters[0]);
        let base_width = f32::from_bits(species.parameters[3]);
        if ![base_height, base_width, height, width]
            .iter()
            .all(|v| v.is_finite())
            || base_height <= 0.
            || base_width <= 0.
        {
            return Err("invalid vegetation proxy placement dimensions".into());
        }
        let width_ratio = width / base_width;
        let height_ratio = height / base_height;
        if ![width_ratio, height_ratio].iter().all(|v| v.is_finite()) {
            return Err("nonfinite vegetation proxy scale ratio".into());
        }
        Ok(Self {
            position: vegetation_collision::placement_position(coordinates, coordinate_unit)?,
            width_ratio,
            height_ratio,
        })
    }
    pub fn from_signed_scales(
        species: &Species,
        coordinates: [u16; 3],
        coordinate_unit: f32,
        height_scale: i8,
        width_scale: i8,
    ) -> Result<Self> {
        let [height, width] = species.dimensions(height_scale, width_scale);
        Self::from_dimensions(species, coordinates, coordinate_unit, height, width)
    }
    /// 001280b0 runtime kind0: retain template basis/W and replace XYZ translation.
    /// The computed width/height ratios are deliberately not applied to meshes.
    pub fn mesh_matrix(self, mut template: [f32; 16]) -> Result<[f32; 16]> {
        if !template
            .iter()
            .chain(self.position.iter())
            .all(|v| v.is_finite())
        {
            return Err("nonfinite vegetation mesh placement matrix".into());
        }
        template[12..15].copy_from_slice(&self.position);
        Ok(template)
    }
}

///Shared proxy definition plus per-plant actor state. Mesh users keep the exact
///constructor geometry and translate it; sphere/box users consume both ratios.
#[derive(Debug, Clone, Copy)]
pub struct PlacedProxy {
    pub record: Record,
    pub transform: PlacementTransform,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn species() -> Species {
        let mut parameters = [0; 23];
        parameters[..6].copy_from_slice(&[10., 8., 4., 2., 1., 3.].map(f32::to_bits));
        Species {
            slot: 0,
            model: String::new(),
            billboard: String::new(),
            parameters,
            optional_name: None,
            proxies: vec![],
        }
    }

    #[test]
    fn record_validation_reads_only_selected_constructor_fields() {
        let mut words = [1, 0, 0, 0, 0x7fc00000, 0xffc00000, 0xdeadbeef, 0, 0];
        assert!(matches!(
            Record::parse(&words).unwrap(),
            Record::Cylinder(_)
        ));
        words[0] = 2;
        words[8] = 0x7fc00000;
        assert!(matches!(Record::parse(&words).unwrap(), Record::Sphere(_)));
        words[0] = 3;
        assert!(Record::parse(&words).is_err());
        words[0] = 0;
        words[1] = 0x7fc00000;
        assert!(matches!(Record::parse(&words).unwrap(), Record::ModelMesh));
        words[0] = 1;
        assert!(Record::parse(&words).is_err());
    }

    #[test]
    fn zero_cylinder_retains_native_faces_and_slot() {
        let mesh = Cylinder {
            center: [1., 2., 3.],
            radius: 0.,
            height: 0.,
        }
        .mesh()
        .unwrap();
        assert!(mesh.vertices.iter().all(|vertex| *vertex == [1., 2., 3.]));
        assert_eq!(mesh.triangles[0], [0, 1, 17]);
        assert_eq!(mesh.triangles[1], [0, 17, 16]);
        assert_eq!(mesh.triangles[30], [15, 0, 16]);
        assert_eq!(mesh.triangles[31], [15, 16, 31]);
        for triangle in mesh.triangles {
            assert!(triangle.iter().any(|index| *index < 16));
            assert!(triangle.iter().any(|index| *index >= 16));
        }
    }

    #[test]
    fn cylinder_vertices_match_native_constructor_polynomial_bits() {
        //Actual001286f0 case 12: includes both sides of the strict pi wrap.
        let words = [
            1, 3234664171, 1088146556, 3220941356, 0x7fc00000, 0xffc00000, 0xdeadbeef, 1077240315,
            1119469935,
        ];
        let Record::Cylinder(cylinder) = Record::parse(&words).unwrap() else {
            panic!()
        };
        let mesh = cylinder.mesh().unwrap();
        assert_eq!(
            mesh.vertices[0].map(f32::to_bits),
            [3227827163, 1120370231, 3220941356]
        );
        assert_eq!(
            mesh.vertices[8].map(f32::to_bits),
            [3239305204, 1120370231, 3220941374]
        );
        assert_eq!(
            mesh.vertices[9].map(f32::to_bits),
            [3239078991, 1120370231, 3225632406]
        );
        assert_eq!(
            mesh.vertices[16].map(f32::to_bits),
            [3227827163, 1088146556, 3220941356]
        );
    }

    #[test]
    fn packed_position_and_signed_dimensions_do_not_scale_mesh_basis() {
        let placement = PlacementTransform::from_signed_scales(
            &species(),
            [65535, 65024, 32768],
            5760. / 65536.,
            -128,
            127,
        )
        .unwrap();
        assert_eq!(placement.position, [5759.912, 5715., 2880.]);
        assert_eq!(placement.height_ratio, 0.8);
        assert_eq!(placement.width_ratio, 1.4960938);
        let template = [
            0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1., 0., 2., 3., 4., 1.,
        ];
        let matrix = placement.mesh_matrix(template).unwrap();
        assert_eq!(&matrix[..12], &template[..12]);
        assert_eq!(matrix[15], template[15]);
        assert_eq!(&matrix[12..15], &placement.position);
        let next =
            PlacementTransform::from_signed_scales(&species(), [0, 0, 0], 1., 127, -128).unwrap();
        let replaced = next.mesh_matrix(matrix).unwrap();
        assert_eq!(&replaced[..12], &template[..12]);
        assert_eq!(&replaced[12..15], &[0., 0., 0.]);
    }

    #[test]
    fn descriptor_kinds_keep_original_runtime_dispatch_tags() {
        for (kind, runtime) in [(0, 0), (1, 0), (2, 5), (3, 4)] {
            let mut words = [0; 9];
            words[0] = kind;
            assert_eq!(Record::parse(&words).unwrap().runtime_actor_kind(), runtime);
        }
    }
}
