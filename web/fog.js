// Original eight-bit GS visibility, carried affinely across the screen. The
// Rust scene reader prepares range/bias; browser clipping/LOD remain separate.
export const fogVertexShader = `
uniform vec2 fogPacking; // encoded bias, distance coefficient
uniform bool fogEnabled;
out vec2 fogInterpolation;
float nativeFogCoverage(float distance) {
  float encoded = clamp(fogPacking.x + fogPacking.y * distance, 524288.0, 524543.0);
  return float((floatBitsToUint(encoded) >> 4u) & 255u) / 255.0;
}
void prepareFog(float distance, vec4 clipPosition) {
  float coverage = fogEnabled ? nativeFogCoverage(distance) : 1.0;
  // Smooth interpolation divides each output by clip W. Carrying both values
  // cancels that division, reproducing the GS screen-linear fog interpolation.
  fogInterpolation = vec2(coverage * clipPosition.w, clipPosition.w);
}
`;
export const fogFragmentShader = `
uniform bool fogEnabled;
in vec2 fogInterpolation;
float fogCoverage() {
  return fogEnabled && fogInterpolation.y != 0.0 ? clamp(fogInterpolation.x / fogInterpolation.y, 0.0, 1.0) : 1.0;
}
`;
