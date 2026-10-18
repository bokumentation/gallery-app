// Preview and export grade pass. Geometry stage (orientation steps, flips,
// free rotation, crop) samples the base texture; the color stage then grades
// in linear light and stores sRGB-encoded RGBA. Slider values arrive in slider
// space from Rust; grade/gpu.rs holds the scaling table.
//
// Pipeline order mirrors the referenced RapidRAW implementation: geometry is
// applied first and the crop lives in pipeline space (orientation steps and
// flips applied), so only the free-rotation step sits between the crop
// rectangle and the base texture.

struct Params {
    // Output pixel size, and the storage-buffer row pitch in bytes.
    out_size: vec2<u32>,
    pitch: u32,
    _pad: u32,
    // Crop rectangle in pipeline-space pixels.
    crop: vec4<f32>,
    // oriented_w, oriented_h, rotation_deg, orientation_steps.
    oriented: vec4<f32>,
    // flip_h, flip_v, pad, pad.
    flips: vec4<f32>,
};

@group(0) @binding(0) var<uniform> par: Params;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> out_buf: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= par.out_size.x || gid.y >= par.out_size.y) {
        return;
    }
    // Crop-space pixel position in pipeline space.
    let position = p_crop_position(vec2<f32>(gid.xy));
    // Undo the free rotation that sits between the crop and the base image.
    let pipeline_pixel = p_undo_rotation(position);
    // Orientation steps and flips map pipeline space onto the texture.
    let texture_pixel = p_pipeline_to_texture(pipeline_pixel);
    let sampled = p_sample_bilinear(texture_pixel);
    let graded = p_grade(sampled);
    let index = gid.y * par.pitch / 4u + gid.x;
    out_buf[index] = p_pack(graded);
}

fn p_crop_position(gid: vec2<f32>) -> vec2<f32> {
    return par.crop.xy + gid + vec2<f32>(0.5, 0.5);
}

fn p_undo_rotation(position: vec2<f32>) -> vec2<f32> {
    if (par.oriented.z == 0.0) {
        return position;
    }
    let center = vec2<f32>(par.oriented.xy) * 0.5;
    let angle = radians(-par.oriented.z);
    let cos = cos(angle);
    let sin = sin(angle);
    let local = position - center;
    let rotated = vec2<f32>(local.x * cos - local.y * sin, local.x * sin + local.y * cos);
    return rotated + center;
}

fn p_pipeline_to_texture(pipeline_pixel: vec2<f32>) -> vec2<f32> {
    let texture_size = vec2<f32>(textureDimensions(src_tex));
    let oriented = p_oriented_size(texture_size);
    var coords = pipeline_pixel;
    if (par.flips.x > 0.5) {
        coords.x = oriented.x - coords.x;
    }
    if (par.flips.y > 0.5) {
        coords.y = oriented.y - coords.y;
    }
    let steps = par.oriented.w;
    if (steps == 1.0) {
        // Pipeline holds the texture rotated 90 degrees clockwise.
        return vec2<f32>(coords.y, oriented.y - coords.x);
    }
    if (steps == 2.0) {
        return vec2<f32>(oriented.x - coords.x, oriented.y - coords.y);
    }
    if (steps == 3.0) {
        return vec2<f32>(oriented.x - coords.y, coords.x);
    }
    return coords;
}

fn p_oriented_size(texture_size: vec2<f32>) -> vec2<f32> {
    if (par.oriented.w == 1.0 || par.oriented.w == 3.0) {
        return texture_size.yx;
    }
    return texture_size;
}

fn p_sample_bilinear(texture_pixel: vec2<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(src_tex));
    let greatest = vec2<f32>(1.0, 1.0);
    let max = dims - greatest;
    let lower = floor(clamp(texture_pixel, vec2<f32>(0.0), max));
    let upper = clamp(lower + vec2<f32>(1.0, 1.0), vec2<f32>(0.0), max);
    let texel = clamp(texture_pixel - lower, vec2<f32>(0.0), greatest);
    let a = textureLoad(src_tex, vec2<i32>(lower), 0);
    let b = textureLoad(src_tex, vec2<i32>(vec2<f32>(upper.x, lower.y)), 0);
    let c = textureLoad(src_tex, vec2<i32>(vec2<f32>(lower.x, upper.y)), 0);
    let d = textureLoad(src_tex, vec2<i32>(upper), 0);
    let top = mix(a, b, vec4<f32>(texel.x));
    let bottom = mix(c, d, vec4<f32>(texel.x));
    return mix(top, bottom, vec4<f32>(texel.y));
}

fn p_grade(sampled: vec4<f32>) -> vec4<f32> {
    // Phase 1: passthrough. The grade lands here in later phases.
    return sampled;
}

fn p_pack(color: vec4<f32>) -> u32 {
    let r = u32(round(clamp(color.r, 0.0, 1.0) * 255.0));
    let g = u32(round(clamp(color.g, 0.0, 1.0) * 255.0));
    let b = u32(round(clamp(color.b, 0.0, 1.0) * 255.0));
    let a = u32(round(clamp(color.a, 0.0, 1.0) * 255.0));
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}
