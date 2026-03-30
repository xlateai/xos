// Per-pixel parallel SDF circles: each thread loads the CPU-uploaded frame, then last-hit wins
// over all circles (same painter order as a forward loop).

struct Params {
    width: u32,
    height: u32,
    count: u32,
    _pad: u32,
}

struct Circle {
    cx: f32,
    cy: f32,
    rad: f32,
    _pad: f32,
    cr: f32,
    cg: f32,
    cb: f32,
    ca: f32,
}

@group(0) @binding(0) var<storage, read> params: Params;
@group(0) @binding(1) var<storage, read> circles: array<Circle>;
@group(0) @binding(2) var input_tex: texture_2d<f32>;
@group(0) @binding(3) var output_tex: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16)
fn cs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let w = params.width;
    let h = params.height;
    if (gid.x >= w || gid.y >= h) {
        return;
    }

    let coord = vec2<i32>(i32(gid.x), i32(gid.y));
    var color = textureLoad(input_tex, coord, 0);

    let px = f32(gid.x);
    let py = f32(gid.y);
    let n = params.count;
    for (var i = 0u; i < n; i++) {
        let c = circles[i];
        let dx = px - c.cx;
        let dy = py - c.cy;
        let r2 = c.rad * c.rad;
        if (dx * dx + dy * dy <= r2) {
            color = vec4<f32>(c.cr, c.cg, c.cb, c.ca);
        }
    }

    textureStore(output_tex, coord, color);
}
