// Conservative presentation-only bounds. Never used for contacts or gameplay.
export function vertexBounds(vertices, stride = 6) {
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < vertices.length; i += stride) for (let a = 0; a < 3; a++) {
    min[a] = Math.min(min[a], vertices[i + a]); max[a] = Math.max(max[a], vertices[i + a]);
  }
  return [...min, ...max].every(Number.isFinite) ? { min, max } : null;
}
export function worldBounds(bounds, matrix) {
  if (!bounds) return null;
  const min = [Infinity, Infinity, Infinity], max = [-Infinity, -Infinity, -Infinity];
  for (let corner = 0; corner < 8; corner++) {
    const p = [0, 1, 2].map(a => (corner & (1 << a)) ? bounds.max[a] : bounds.min[a]);
    for (let a = 0; a < 3; a++) {
      const v = matrix[a] * p[0] + matrix[4 + a] * p[1] + matrix[8 + a] * p[2] + matrix[12 + a];
      min[a] = Math.min(min[a], v); max[a] = Math.max(max[a], v);
    }
  }
  return [...min, ...max].every(Number.isFinite) ? { min, max } : null;
}
export function viewPlanes(matrix) {
  const planes = [];
  for (const row of [0, 1, 2]) for (const sign of [1, -1]) {
    const p = [0, 1, 2, 3].map(c => matrix[c * 4 + 3] + sign * matrix[c * 4 + row]);
    const length = Math.hypot(p[0], p[1], p[2]);
    if (!(length > 0) || !p.every(Number.isFinite)) return [];
    planes.push(p.map(v => v / length));
  }
  return planes;
}
export function outsideView(bounds, planes) {
  if (!bounds) return false;
  for (const p of planes) {
    // Most-positive box corner. A box intersecting any clip plane stays drawn.
    let distance = p[3], magnitude = Math.abs(p[3]);
    for (let a = 0; a < 3; a++) {
      const v = p[a] * (p[a] >= 0 ? bounds.max[a] : bounds.min[a]);
      distance += v; magnitude += Math.abs(v);
    }
    // Allow for float32 GPU transforms at the original large world coordinates.
    if (distance < -(0.05 + magnitude * 1e-6)) return true;
  }
  return false;
}
