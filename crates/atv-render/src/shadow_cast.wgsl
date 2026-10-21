// High preset vehicle shadow map casters (VehicleShadowMap in
// web/shadow-map.js): depth only, posed by the vehicle bone uniforms.
@group(0) @binding(0) var<uniform> light: mat4x4<f32>;
struct Bones {
  world: mat4x4<f32>,
  nodes: array<mat4x4<f32>, 32>,
};
@group(1) @binding(0) var<uniform> bones: Bones;

@vertex
fn vs(@location(0) position: vec3<f32>, @location(3) node: f32) -> @builtin(position) vec4<f32> {
  return light * (bones.world * (bones.nodes[u32(node)] * vec4<f32>(position, 1.0)));
}

@fragment
fn fs() {}
