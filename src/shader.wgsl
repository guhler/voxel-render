const XZ: u32 = 128;
const Y: u32 = 64;

const NUM_XZ_VERTICES: u32 = XZ * 4;
const NUM_Y_VERTICES: u32 = Y * 4;

const AXIS_X: u32 = 0;
const AXIS_Y: u32 = 1;
const AXIS_Z: u32 = 2;

const FACE_FRONT: bool = false;
const FACE_BACK: bool = true;

const CHUNK_SIZE: u32 = 32;
const CHUNK_STRIDE: u32 = XZ / CHUNK_SIZE;
const CHUNK_COUNT: u32 = CHUNK_STRIDE * CHUNK_STRIDE;
const CHUNK_ARRAY_SIZE: u32 = CHUNK_COUNT * CHUNK_SIZE * CHUNK_SIZE / 4;

const epsilon: f32 = 0.001;

struct GlobalsUniform {
    proj_view_mat: mat4x4<f32>,
    light_mat: mat4x4<f32>,
    cam_pos: vec3<f32>,
    cam_dir: vec3<f32>,
    light_pos: vec3<f32>,
    light_dir: vec3<f32>,
    grid_lines: u32,
}

@group(0) @binding(0)
var<uniform> globals: GlobalsUniform;

@group(0) @binding(1)
var blocks: texture_3d<u32>;

@group(1) @binding(0)
var shadow_map: texture_depth_2d;

@group(1) @binding(1)
var shadow_sampler: sampler_comparison;

struct VertexInput {
    @location(0) pos: vec3<f32>,
    @location(1) axis: u32,
}

struct VertexOutput {
    @builtin(position) pos: vec4<f32>,
    @location(0) world_pos: vec3<f32>,
    @location(1) light_pos: vec4<f32>,
    @location(2) axis: u32,
}

@vertex
fn vx_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.pos = globals.proj_view_mat * vec4<f32>(in.pos, 1.0);
    out.world_pos = in.pos;

    let light_pos = globals.light_mat * vec4<f32>(in.pos, 1.0);
    out.light_pos = light_pos;
    out.axis = in.axis;
    return out;
}

@vertex
fn vx_shadow(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    let light_pos = globals.light_mat * vec4<f32>(in.pos, 1.0);
    out.pos = light_pos;
    out.world_pos = in.pos;
    out.axis = in.axis;
    return out;
}

fn get_block(coord: vec3<f32>, axis: u32, face: bool) -> bool {
    let offset = select(0.0, -1.0, face == FACE_BACK);
    var coord_i: vec3<i32>;

    if axis == AXIS_X {
        coord_i.x = i32(round(coord.x + offset));
        coord_i.y = i32(floor(coord.y));
        coord_i.z = i32(floor(coord.z));
    } else if axis == AXIS_Y {
        coord_i.x = i32(floor(coord.x));
        coord_i.y = i32(round(coord.y + offset));
        coord_i.z = i32(floor(coord.z));
    } else {
        coord_i.x = i32(floor(coord.x));
        coord_i.y = i32(floor(coord.y));
        coord_i.z = i32(round(coord.z + offset));
    }

    if coord_i.x < -i32(XZ / 2) || coord_i.x >= i32(XZ)
        || coord_i.z < -i32(XZ / 2) || coord_i.z >= i32(XZ)
        || coord_i.y < -i32(XZ / 2) || coord_i.y >= i32(Y)
    {
        return false;
    }

    let tx = textureLoad(
        blocks,
        vec3<i32>(
            coord_i.x + i32(XZ / 2),
            coord_i.z + i32(XZ / 2),
            coord_i.y + i32(Y / 2),
        ),
        0,
    );
    return bool(tx[0]);
}

struct FragmentInput {
    @location(0) world_pos: vec3<f32>,
    @location(1) light_pos: vec4<f32>,
    @location(2) axis: u32,
}

fn block_color(coord: vec3<f32>, axis: u32, face: bool) -> vec4<f32> {
    let block = get_block(coord, axis, face);
    if !block {
        return vec4<f32>(0.0);
    }

    var normal: vec3<f32>;
    if axis == AXIS_X {
        normal = vec3<f32>(1.0, 0.0, 0.0);
    } else if axis == AXIS_Y {
        normal = vec3<f32>(0.0, 1.0, 0.0);
    } else {
        normal = vec3<f32>(0.0, 0.0, 1.0);
    }
    if face == FACE_BACK {
        normal *= -1.0;
    }

    let exposed = !get_block(coord - normal, axis, face);
    if !exposed {
        return vec4<f32>(0.0);
    }

    let dot = dot(globals.light_dir, normal) + 0.3;
    return vec4<f32>(vec3<f32>(dot), 1.0);
}

fn grid_dist(v: f32, size: f32) -> f32 {
    let dist = fract(v / size);
    return min(dist, 1.0 - dist) * size;
}

fn chunk_grid(coord: vec3<f32>, axis: u32, face: bool) -> bool {
    let thickness = 2.0;

    let dist = max(
        grid_dist(coord.x, f32(CHUNK_SIZE)),
        max(
            grid_dist(coord.y, f32(CHUNK_SIZE)),
            grid_dist(coord.z, f32(CHUNK_SIZE)),
        ),
    );

    let w = fwidth(coord.y);
    let l = smoothstep(0.0, w * thickness, dist);

    return l < 0.9;

    //  let grid_size = 32.0;

    //  let dx4 = grid_dist(coord.x, 4.0);
    //  let dy4 = grid_dist(coord.y, 4.0);
    //  let dz4 = grid_dist(coord.z, 4.0);
    //  let dx32 = grid_dist(coord.x, 32.0);
    //  let dz32 = grid_dist(coord.z, 32.0);
    //  let dist_l1 = length(vec2<f32>(dx4, dz32));
    //  let dist_l2 = length(vec2<f32>(dx32, dz4));
    //  let dist_l3 = length(vec2<f32>(dy4, dx32));
    //  let dist_l4 = length(vec2<f32>(dy4, dz32));

    //  let w = fwidth(coord.y);

    //  let l1 = step(dist_l1, w * thickness);
    //  let l2 = step(dist_l2, w * thickness);
    //  let l3 = step(dist_l3, w * thickness);
    //  let l4 = step(dist_l4, w * thickness);

    //  let line = max(l1, max(l2, max(l3, l4)));

    //  return line == 1.0;
}

@fragment
fn fg_main(in: FragmentInput) -> @location(0) vec4<f32> {

    let face = select(FACE_FRONT, FACE_BACK,
        (in.axis == AXIS_X && in.world_pos.x < globals.cam_pos.x)
        || (in.axis == AXIS_Y && in.world_pos.y < globals.cam_pos.y)
        || (in.axis == AXIS_Z && in.world_pos.z < globals.cam_pos.z)
    );

    if globals.grid_lines != 0 && chunk_grid(in.world_pos, in.axis, face) {
        return vec4<f32>(1.0, 1.0, 0.0, 1.0);
    }

    var block_color = block_color(in.world_pos, in.axis, face);

    if block_color.w == 0.0 {
        discard;
    }

    let uv = vec2<f32>(
        (in.light_pos.x / in.light_pos.w) * 0.5 + 0.5,
        -(in.light_pos.y / in.light_pos.w) * 0.5 + 0.5,
    );
    let depth = in.light_pos.z / in.light_pos.w;

    let shadow_depth = textureSampleCompare(
        shadow_map,
        shadow_sampler,
        uv,
        depth,
    );

    block_color *= shadow_depth;

    return block_color;
}

@fragment
fn fg_shadow(in: FragmentInput) {
    let face = select(FACE_FRONT, FACE_BACK,
        (in.axis == AXIS_X && in.world_pos.x < globals.light_pos.x)
        || (in.axis == AXIS_Y && in.world_pos.y < globals.light_pos.y)
        || (in.axis == AXIS_Z && in.world_pos.z < globals.light_pos.z)
    );

    let block = get_block(in.world_pos, in.axis, face);
    if !block {
        discard;
    }

    var normal: vec3<f32>;
    if in.axis == AXIS_X {
        normal = vec3<f32>(1.0, 0.0, 0.0);
    } else if in.axis == AXIS_Y {
        normal = vec3<f32>(0.0, 1.0, 0.0);
    } else {
        normal = vec3<f32>(0.0, 0.0, 1.0);
    }
    if face == FACE_BACK {
        normal *= -1.0;
    }

    let exposed = !get_block(in.world_pos - normal, in.axis, face);
    if !exposed {
        discard;
    }
}

