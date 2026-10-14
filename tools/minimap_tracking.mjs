// Observe canvas crops and actual GPU actor transforms without altering inputs,
// camera, simulation or renderer state. Checks run only on fresh GPS updates.
export async function captureMinimapTracking(page) {
  await page.addInitScript(() => {
    window.minimapTracking = { samples: [], errors: [] };
    const names = new WeakMap(), p = WebGL2RenderingContext.prototype;
    const get = p.getUniformLocation, upload = p.uniformMatrix4fv;
    const draw = CanvasRenderingContext2D.prototype.drawImage;
    let model, crop, checked = -1;
    p.getUniformLocation = function (program, name) {
      const location = get.call(this, program, name); if (location) names.set(location, name); return location;
    };
    CanvasRenderingContext2D.prototype.drawImage = function (...args) {
      if (this.canvas.parentElement?.id === 'freestyle-minimap' && args.length === 9) crop = args.slice(1, 5);
      return draw.apply(this, args);
    };
    p.uniformMatrix4fv = function (location, transpose, value, ...rest) {
      const name = names.get(location);
      if (name === 'model') model = Array.from(value);
      if (name?.startsWith('quadBones')) {
        const s = window.atvTerrainStatus, map = s?.minimap, frame = s?.frame;
        if (map?.visible && map.updates !== checked && frame && crop) {
          const matrices = map.actor === 'ragdoll rider' ? frame.funny_crash?.rider_node_matrices
            : map.actor === 'rider' ? frame.rider_node_matrices : frame.quad_node_matrices;
          const matches = matrices?.length * 16 === value.length && matrices.every((m, i) => m.every((x, j) => Math.fround(x) === value[i * 16 + j]));
          // Motorcycle bone blends use their rendered origin and model matrix.
          const bike = map.actor === 'vehicle' && s.motorcyclePresentation && model
            && s.motorcyclePresentation.origin.every((x, i) => Math.abs(x - model[12 + i]) < .001);
          if (matches || bike) {
            const m = map.actor === 'ragdoll rider' ? value : model;
            const origin = [m[12], m[14]], heading = Math.atan2(m[8], m[10]);
            const positionError = Math.hypot(map.center[0] - origin[0], map.center[1] - origin[1]);
            const headingError = Math.abs(Math.atan2(Math.sin(map.heading - heading), Math.cos(map.heading - heading)));
            const width = (s.terrainGrid[0] - 1) * s.terrainSampleSpacing, depth = (s.terrainGrid[1] - 1) * s.terrainSampleSpacing;
            // Independent overhead basis: screen-right=+X, screen-up=+Z.
            const cropWorld = [width * ((crop[0] + crop[2] / 2) / 1024), depth * (1 - (crop[1] + crop[3] / 2) / 1024)];
            const cropError = Math.hypot(cropWorld[0] - origin[0], cropWorld[1] - origin[1]);
            const sample = { actor: map.actor, positionError, headingError, cropError, zoom: map.zoom, updates: map.updates };
            minimapTracking.samples.push(sample); checked = map.updates;
            if (positionError > .002 || headingError > .0001 || cropError > .002) minimapTracking.errors.push(sample);
          }
        }
      }
      return upload.call(this, location, transpose, value, ...rest);
    };
  });
}
