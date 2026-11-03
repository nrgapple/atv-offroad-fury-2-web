//! The garage's lean vehicle load gives what the full loaders give: every
//! member through the expanded archive, the bike and rider through
//! `motorcycle::Selection`, the textures through decode jobs. Needs the
//! private hosted content package:
//!   FURY_CONTENT=private/reports/hosted-content cargo test -p atv-game \
//!     --test garage_vehicle_load -- --ignored
use atv_content::{Content, Store};
use atv_core::{motorcycle, resource::Resource};
use atv_game::ride::{load::expanded_archive, textures};
use std::collections::BTreeMap;
use std::future::Future;
use std::task::{Context, Poll, Waker};

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    loop {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return value;
        }
    }
}

type Pixels = BTreeMap<String, (Option<(u32, u32, Vec<u8>)>, bool, bool, bool)>;

fn pixels(textures: std::collections::HashMap<String, textures::MaterialTexture>) -> Pixels {
    let image = |i: atv_game::venue::Image| (i.width, i.height, i.rgba);
    textures
        .into_iter()
        .map(|(name, t)| {
            let mud = t.mud.is_some();
            (name, (t.image.map(image), t.fog, mud, t.blend))
        })
        .collect()
}

#[test]
#[ignore = "requires the private hosted content package (FURY_CONTENT)"]
fn the_lean_garage_load_matches_the_full_loaders() {
    let dir = std::env::var_os("FURY_CONTENT").expect("set FURY_CONTENT");
    block_on(async {
        let content = Content::open(Store::Directory(dir.into())).await.unwrap();
        // One bike of each archive, and an ATV of each bank.
        for quad in [27u16, 29, 31, 33, 1, 14] {
            let models = atv_core::player_models::selection(quad, 0).unwrap();
            let original = content.file(&models.quad).await.unwrap();
            let expanded = expanded_archive(&content, &models.quad).await.unwrap();
            let (a, b) = (
                Resource::parse(&original).unwrap(),
                Resource::parse(&expanded).unwrap(),
            );
            assert!(!b.compressed, "{quad}: expanded");
            assert_eq!(a.members.len(), b.members.len());
            for i in 0..a.members.len() {
                assert_eq!(a.extract(i).unwrap(), b.extract(i).unwrap(), "{quad}: {i}");
            }
            if !models.motorcycle {
                let materials = content.file(&models.quad_materials).await.unwrap();
                let materials = Resource::parse(&materials).unwrap();
                let index = a
                    .members
                    .iter()
                    .position(|m| {
                        m.name
                            .rsplit('/')
                            .next()
                            .is_some_and(|n| n.eq_ignore_ascii_case(&models.quad_mesh))
                    })
                    .unwrap();
                let mesh = atv_core::mesh::decode_psg(&a.extract(index).unwrap()).unwrap();
                assert_eq!(
                    pixels(textures::load(&mesh, &materials).unwrap()),
                    pixels(textures::load_jobs(&mesh, &materials).await.unwrap()),
                    "{quad}: textures"
                );
                continue;
            }
            let model = motorcycle::Model::load(&original, quad).unwrap();
            let selection = motorcycle::Selection::load(&expanded, quad).unwrap();
            assert_eq!(format!("{:?}", model.mesh), format!("{:?}", selection.mesh));
            assert_eq!(
                format!("{:?}", model.rider),
                format!("{:?}", selection.rider)
            );
            assert_eq!(model.pose(0., [0., 0.], [0., 0.]), selection.pose);
            assert_eq!(selection.motions.len(), 4, "{quad}: the selection clips");
            for (name, clip) in &selection.motions {
                assert_eq!(
                    format!("{:?}", model.motions[name]),
                    format!("{clip:?}"),
                    "{quad}: {name}"
                );
            }
            for mesh in [&selection.mesh, &selection.rider] {
                assert_eq!(
                    pixels(textures::load(mesh, &a).unwrap()),
                    pixels(textures::load_jobs(mesh, &b).await.unwrap()),
                    "{quad}: textures"
                );
            }
        }
    });
}
