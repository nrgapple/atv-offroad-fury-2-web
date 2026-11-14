//! Count-gated MX PhysicsCollections (002171b0, 004922a0). The shared static
//! world uses each active model's F collision target at its authored pose.
//! M files contain actor whiskers, not static geometry. Movable rigid-body
//! dynamics remain separate from this explicit static compatibility adapter.
use crate::{
    Result,
    scene::{self, CollisionModel, StaticModel},
};

pub fn targets(toy: &[u8], models: &[StaticModel]) -> Result<Vec<CollisionModel>> {
    let data = scene::document(toy)?;
    let count = data
        .first("PhysicsCollections", "NumberOfPhysicsCollections")
        .map_or(Ok(0), |entry| {
            entry
                .value
                .parse::<usize>()
                .map_err(|_| "invalid MX collection count".to_string())
        })?;
    if count > 256 || models.len() > 8192 {
        return Err("MX collection count exceeds bound".into());
    }
    let mut active = vec![false; models.len()];
    for index in 1..=count {
        let pattern = data
            .first(&format!("PhysicsCollection{index}"), "Name")?
            .value
            .trim_matches('"')
            .to_ascii_lowercase();
        if pattern.is_empty()
            || pattern.len() > 128
            || pattern.contains(['/', '\\', ':'])
            || pattern.matches('*').count() > 1
            || (pattern.contains('*') && !pattern.ends_with('*'))
        {
            return Err("unsupported MX collection model pattern".into());
        }
        for (slot, model) in models.iter().enumerate() {
            let name = model.name.to_ascii_lowercase();
            active[slot] |= match pattern.strip_suffix('*') {
                Some(prefix) => name.starts_with(prefix),
                None => name == pattern,
            };
        }
    }
    Ok(models
        .iter()
        .zip(active)
        .filter(|(_, active)| *active)
        .map(|(model, _)| CollisionModel {
            name: format!("{}f", model.name),
            world_matrix: model.world_matrix,
            material: 0,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(name: &str, x: f32) -> StaticModel {
        let mut matrix = [0.; 16];
        for i in [0, 5, 10, 15] {
            matrix[i] = 1.;
        }
        matrix[12] = x;
        StaticModel {
            name: name.into(),
            position: [x, 0., 0.],
            look: [0., 0., 1.],
            up: None,
            use_lighting: true,
            dynamic_lighting: false,
            world_matrix: matrix,
        }
    }
    #[test]
    fn count_gate_wildcards_and_instances_preserve_independent_f_targets() {
        let models = [
            model("BaleA", 1.),
            model("BaleA", 4.),
            model("BaleB", 9.),
            model("Mailbox", 0.),
        ];
        let source = b"[PhysicsCollections]\nNumberOfPhysicsCollections=2\n[PhysicsCollection1]\nName=Bale*\n[PhysicsCollection2]\nName=BaleA\n[PhysicsCollection3]\nName=Mailbox\n";
        let result = targets(source, &models).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].name, "BaleAf");
        assert_eq!(result[1].world_matrix[12], 4.);
        assert_eq!(result[2].name, "BaleBf");
        assert!(targets(b"[PhysicsCollections]\nNumberOfPhysicsCollections=0\n[PhysicsCollection1]\nName=Mailbox", &models).unwrap().is_empty());
        assert!(
            targets(
                b"[PhysicsCollections]\nNumberOfPhysicsCollections=257",
                &models
            )
            .is_err()
        );
    }
}
