// Original train meshes and Rust route/MOT poses. Geometry is shared between
// repeated authored cars; scene time and heading never advance in JavaScript.
const identity = new Float32Array([1,0,0,0,0,1,0,0,0,0,1,0,0,0,0,1]);
const normalize = name => name.replaceAll('\\', '/').toLowerCase();
const meshName = name => `geometry/${normalize(name).replace(/\.slt$/, '.psg')}`;
export async function loadTrain({ resource, entries, modeIndex, codec, json, baseTextures, isCurrent }) {
  if (modeIndex < 0) return null;
  const frame = await json(67, resource, modeIndex);
  if (!frame) return null;
  const textures = new Map(), meshes = new Map();
  for (const name of new Set(frame.cars.map(car => meshName(car.model)).concat('anim/loco_wheels.psg'))) {
    if (!isCurrent()) throw new Error('Opening cancelled.');
    const index = entries.findIndex(entry => normalize(entry.name) === name);
    if (index < 0) throw new Error(`Original train mesh is missing: ${name}`);
    const mesh = await json(11, await codec(3, resource, index));
    meshes.set(name, mesh);
    await baseTextures(mesh, resource, entries, textures);
  }
  return { resource, modeIndex, frame, meshes, textures };
}
export class TrainRenderer {
  constructor(gl, source, { upload, texture, inverseDirection }) {
    this.gl = gl; this.source = source; this.chunks = []; this.textures = []; this.geometry = new Map(); this.draws = 0; this.triangles = 0;
    const textures = new Map();
    for (const [name, image] of source.textures) if (image.rgba) { const gpu = texture(image); textures.set(name, gpu); this.textures.push(gpu); }
    for (const [name, mesh] of source.meshes) {
      if (mesh.nodes.length > 32) throw new Error('Train model exceeds original pose capacity.');
      const chunks = [];
      for (const group of mesh.lods[0].groups) {
        if (group.vertices.length > 65536) throw new Error('Train mesh group exceeds index capacity.');
        const material = mesh.materials[group.material], image = source.textures.get(material);
        if (image?.reflective) continue;
        const vertices = new Float32Array(group.vertices.length * 9);
        group.vertices.forEach((vertex, index) => {
          const node = mesh.nodes[vertex.node];
          if (!node) throw new Error('Train mesh bone is missing.');
          vertices.set([...vertex.local_position, ...inverseDirection(node.world_matrix, vertex.normal), ...vertex.uv, vertex.node], index * 9);
        });
        const chunk = upload(vertices, new Uint16Array(group.indices), true, true);
        chunk.texture = textures.get(material); chunk.alphaCutoff = image?.sceneAlphaCutoff ?? 0;
        chunk.fog = image?.pass?.fog ?? true;
        chunks.push(chunk); this.chunks.push(chunk);
      }
      this.geometry.set(name, { chunks, bind: new Float32Array(mesh.nodes.flatMap(node => node.world_matrix)) });
    }
    this.accept(source.frame);
  }
  accept(frame) {
    if (!frame || !Number.isFinite(frame.time) || !Number.isFinite(frame.duration) || frame.cars.some(car => car.matrix.length !== 16 || !car.matrix.every(Number.isFinite)) || (frame.wheel_matrices && frame.wheel_matrices.some(matrix => matrix.length !== 16 || !matrix.every(Number.isFinite)))) throw new Error('Simulation returned an invalid train pose.');
    this.frame = frame; this.source.frame = frame;
  }
  draw(uniforms, fallbackTexture, fogEnabled = false) {
    const gl = this.gl; this.draws = 0; this.triangles = 0;
    gl.uniform1i(uniforms.useQuadPose, true); gl.uniform1i(uniforms.useEcosystem, false); gl.uniform1i(uniforms.useTerrainTexture, false);
    const draw = (geometry, matrix, bones, unlit) => {
      if (!geometry) return;
      gl.uniformMatrix4fv(uniforms.model, false, matrix); gl.uniformMatrix4fv(uniforms.quadBones, false, bones); gl.uniform1i(uniforms.unlit, unlit);
      for (const chunk of geometry.chunks) {
        gl.uniform1i(uniforms.fogEnabled,fogEnabled&&chunk.fog);
        gl.uniform1f(uniforms.alphaCutoff, chunk.alphaCutoff); gl.uniform1i(uniforms.useTexture, Boolean(chunk.texture)); gl.bindTexture(gl.TEXTURE_2D, chunk.texture ?? fallbackTexture);
        gl.bindVertexArray(chunk.vao); gl.drawElements(gl.TRIANGLES, chunk.count, gl.UNSIGNED_SHORT, 0); this.draws++; this.triangles += chunk.count / 3;
      }
    };
    for (const car of this.frame.cars) {
      const geometry = this.geometry.get(meshName(car.model));
      draw(geometry, new Float32Array(car.matrix), geometry?.bind, !car.use_lighting);
    }
    if (this.frame.wheel_matrices) draw(this.geometry.get('anim/loco_wheels.psg'), identity, new Float32Array(this.frame.wheel_matrices.flat()), false);
    gl.uniform1i(uniforms.useQuadPose, false); gl.uniform1f(uniforms.alphaCutoff, 0);
  }
  dispose() {
    const gl = this.gl;
    for (const chunk of this.chunks) { gl.deleteVertexArray(chunk.vao); gl.deleteBuffer(chunk.vertices); gl.deleteBuffer(chunk.indices); }
    for (const texture of this.textures) gl.deleteTexture(texture);
    this.chunks = []; this.textures = [];
  }
}
