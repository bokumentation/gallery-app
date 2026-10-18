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

// Per-range HSL mixer values (hue/saturation/luminance), 16-byte aligned as a
// uniform array element requires.
struct HslColor {
    hue: f32,
    saturation: f32,
    luminance: f32,
    _pad: f32,
};

// One color-grading wheel: hue degrees, saturation and luminance in shader
// units (see grade/gpu.rs for the SCALES mirror).
struct ColorGradeSettings {
    hue: f32,
    saturation: f32,
    luminance: f32,
    _pad: f32,
};

// Color grading parameters, pre-scaled in Rust (grade/gpu.rs holds the
// scaling table mirrored from the reference implementation's SCALES map).
// Byte offsets are mirrored by the offset constants in grade/gpu.rs; the
// layout is `vec3<f32>` = 12 bytes, `mat3x3<f32>` = 3x16 bytes, uniform array
// stride 16.
struct ColorParams {
    exposure: f32,
    brightness: f32,
    contrast: f32,
    highlights: f32,
    shadows: f32,
    whites: f32,
    blacks: f32,
    hue: f32,
    rgb_to_lms: mat3x3<f32>,
    lms_to_rgb: mat3x3<f32>,
    wb_gains: vec3<f32>,
    saturation: f32,
    vibrance: f32,
    has_lut: f32,
    // Uniform array members must start on a 16-byte boundary.
    @align(16) hsl: array<HslColor, 8>,
    @align(16) color_grading: array<ColorGradeSettings, 4>,
    grading_blending: f32,
    grading_balance: f32,
};

@group(0) @binding(0) var<uniform> par: Params;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var<uniform> col: ColorParams;
@group(0) @binding(3) var<storage, read_write> out_buf: array<u32>;
@group(0) @binding(4) var src_lut: texture_3d<f32>;
@group(0) @binding(5) var lut_sampler: sampler;

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
    let graded = p_grade(sampled, gid.xy);
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
        // Pipeline holds the texture rotated 90 degrees clockwise. The source
        // width is the oriented height, so the inverse subtracts oriented.x.
        return vec2<f32>(coords.y, oriented.x - coords.x);
    }
    if (steps == 2.0) {
        return vec2<f32>(oriented.x - coords.x, oriented.y - coords.y);
    }
    if (steps == 3.0) {
        // 90 degrees counter-clockwise: the source width is the oriented
        // height, so the inverse subtracts oriented.y.
        return vec2<f32>(oriented.y - coords.y, coords.x);
    }
    return coords;
}

fn p_oriented_size(texture_size: vec2<f32>) -> vec2<f32> {
    if (par.oriented.w == 1.0 || par.oriented.w == 3.0) {
        return texture_size.yx;
    }
    return texture_size;
}

// `texture_pixel` is in pixel-corner space (texel centres at i + 0.5); shift to
// texel-index space before the bilinear taps so identity lands exactly on the
// matching texel instead of halfway between two.
fn p_sample_bilinear(texture_pixel: vec2<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(src_tex));
    let max = dims - vec2<f32>(1.0, 1.0);
    let p = clamp(texture_pixel - vec2<f32>(0.5, 0.5), vec2<f32>(0.0), max);
    let lower = floor(p);
    let upper = clamp(lower + vec2<f32>(1.0, 1.0), vec2<f32>(0.0), max);
    let texel = clamp(p - lower, vec2<f32>(0.0), vec2<f32>(1.0, 1.0));
    let a = textureLoad(src_tex, vec2<i32>(lower), 0);
    let b = textureLoad(src_tex, vec2<i32>(vec2<f32>(upper.x, lower.y)), 0);
    let c = textureLoad(src_tex, vec2<i32>(vec2<f32>(lower.x, upper.y)), 0);
    let d = textureLoad(src_tex, vec2<i32>(upper), 0);
    let top = mix(a, b, vec4<f32>(texel.x));
    let bottom = mix(c, d, vec4<f32>(texel.x));
    return mix(top, bottom, vec4<f32>(texel.y));
}

fn p_grade(sampled: vec4<f32>, coords: vec2<u32>) -> vec4<f32> {
    // Non-RAW input: decode sRGB to linear, grade in linear light, re-encode,
    // then the filmic brightness on the encoded tone. Order mirrors the
    // reference implementation's main().
    var linear = p_srgb_to_linear(max(sampled.rgb, vec3<f32>(0.0)));
    linear = p_linear_exposure(linear);
    linear = p_white_balance(linear);
    linear = p_tonal(linear);
    linear = p_highlights(linear);
    linear = p_hsl(linear);
    linear = p_hue_shift(linear);
    linear = p_creative_color(linear);
    linear = p_color_grading(linear);
    var srgb = p_linear_to_srgb(linear);
    srgb = p_filmic_exposure(srgb);
    srgb = p_lut(srgb);
    // Dither before quantization guards against banding.
    srgb += dither(coords) * (1.0 / 255.0);
    return vec4<f32>(srgb, sampled.a);
}

// Trilinear lookup in display space (sRGB-encoded); neutral-tonemap LUTs map
// 1:1 through the linear-filtered 3D texture.
fn p_lut(color: vec3<f32>) -> vec3<f32> {
    if (col.has_lut < 0.5) {
        return color;
    }
    return textureSampleLevel(src_lut, lut_sampler, color, 0.0).rgb;
}

fn p_pack(color: vec4<f32>) -> u32 {
    let r = u32(round(clamp(color.r, 0.0, 1.0) * 255.0));
    let g = u32(round(clamp(color.g, 0.0, 1.0) * 255.0));
    let b = u32(round(clamp(color.b, 0.0, 1.0) * 255.0));
    let a = u32(round(clamp(color.a, 0.0, 1.0) * 255.0));
    return r | (g << 8u) | (b << 16u) | (a << 24u);
}

// ---------------------------------------------------------------------------
// Color grading, ported from the referenced RapidRAW shader with the local
// detail terms held at zero (single-pass preview and export; no detail
// pyramid), keeping the slider semantics identical.
// ---------------------------------------------------------------------------

const LUMA_COEFF = vec3<f32>(0.2126, 0.7152, 0.0722);
const GF_REINJECT_SIGMA: f32 = 1.0;

fn get_luma(c: vec3<f32>) -> f32 {
    return dot(c, LUMA_COEFF);
}

fn p_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.04045);
    let a = vec3<f32>(0.055);
    let higher = pow((c + a) / (1.0 + a), vec3<f32>(2.4));
    let lower = c / 12.92;
    return select(higher, lower, c <= cutoff);
}

fn p_linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let safe_c = max(c, vec3<f32>(0.0));
    let cutoff = vec3<f32>(0.0031308);
    let a = vec3<f32>(0.055);
    let higher = (1.0 + a) * pow(safe_c, vec3<f32>(1.0 / 2.4)) - a;
    let lower = safe_c * 12.92;
    return select(higher, lower, safe_c <= cutoff);
}

fn linear_to_srgb_extended(c: vec3<f32>) -> vec3<f32> {
    return p_linear_to_srgb(c);
}

fn hash(p: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(p.xyx) * 0.1031);
    p3 += dot(p3, p3.yzx + vec2<f32>(33.33, 33.33).xxy);
    return fract((p3.x + p3.y) * p3.z);
}

fn gradient_noise(p: vec2<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);

    let ga = vec2<f32>(hash(i), hash(i + vec2<f32>(11.0, 37.0))) * 2.0 - 1.0;
    let gb = vec2<f32>(hash(i + vec2<f32>(1.0, 0.0)), hash(i + vec2<f32>(12.0, 37.0))) * 2.0 - 1.0;
    let gc = vec2<f32>(hash(i + vec2<f32>(0.0, 1.0)), hash(i + vec2<f32>(11.0, 38.0))) * 2.0 - 1.0;
    let gd = vec2<f32>(hash(i + vec2<f32>(1.0, 1.0)), hash(i + vec2<f32>(12.0, 38.0))) * 2.0 - 1.0;

    let dot_00 = dot(ga, f);
    let dot_10 = dot(gb, f - vec2<f32>(1.0, 0.0));
    let dot_01 = dot(gc, f - vec2<f32>(0.0, 1.0));
    let dot_11 = dot(gd, f - vec2<f32>(1.0, 1.0));

    let bottom_interp = mix(dot_00, dot_10, u.x);
    let top_interp = mix(dot_01, dot_11, u.x);
    return mix(bottom_interp, top_interp, u.y);
}

fn dither(coords: vec2<u32>) -> f32 {
    return gradient_noise(vec2<f32>(coords));
}

fn rgb_to_hsv(c: vec3<f32>) -> vec3<f32> {
    let c_max = max(c.r, max(c.g, c.b));
    let c_min = min(c.r, min(c.g, c.b));
    let delta = c_max - c_min;
    var h: f32 = 0.0;
    if (delta > 0.0) {
        if (c_max == c.r) {
            h = 60.0 * (((c.g - c.b) / delta) % 6.0);
        } else if (c_max == c.g) {
            h = 60.0 * (((c.b - c.r) / delta) + 2.0);
        } else {
            h = 60.0 * (((c.r - c.g) / delta) + 4.0);
        }
    }
    if (h < 0.0) {
        h += 360.0;
    }
    let s = select(0.0, delta / c_max, c_max > 0.0);
    return vec3<f32>(h, s, c_max);
}

fn hsv_to_rgb(c: vec3<f32>) -> vec3<f32> {
    let h = c.x;
    let s = c.y;
    let v = c.z;
    let big_c = v * s;
    let x = big_c * (1.0 - abs((h / 60.0) % 2.0 - 1.0));
    let m = v - big_c;
    var rgb_prime: vec3<f32>;
    if (h < 60.0) {
        rgb_prime = vec3<f32>(big_c, x, 0.0);
    } else if (h < 120.0) {
        rgb_prime = vec3<f32>(x, big_c, 0.0);
    } else if (h < 180.0) {
        rgb_prime = vec3<f32>(0.0, big_c, x);
    } else if (h < 240.0) {
        rgb_prime = vec3<f32>(0.0, x, big_c);
    } else if (h < 300.0) {
        rgb_prime = vec3<f32>(x, 0.0, big_c);
    } else {
        rgb_prime = vec3<f32>(big_c, 0.0, x);
    }
    return rgb_prime + vec3<f32>(m, m, m);
}

// Linear exposure: pure gain, applied first in linear light.
fn p_linear_exposure(color: vec3<f32>) -> vec3<f32> {
    if (col.exposure == 0.0) {
        return color;
    }
    return color * pow(2.0, col.exposure);
}

// Chromatic adaptation through Bradford cone response: white stays white and
// every color shifts by the same log gains computed in Rust.
fn p_white_balance(color: vec3<f32>) -> vec3<f32> {
    if (all(col.wb_gains == vec3<f32>(0.0))) {
        return color;
    }
    let gains = exp(col.wb_gains);
    let white = col.lms_to_rgb * (gains * (col.rgb_to_lms * vec3<f32>(1.0)));
    return col.lms_to_rgb * (gains * (col.rgb_to_lms * color)) / get_luma(white);
}

// Contrast, shadows, whites and blacks in linear light. The local detail
// restoration terms of the reference (local/mid log detail) are held at 0,
// which collapses their correction factors to exactly 1.
fn p_tonal(color: vec3<f32>) -> vec3<f32> {
    var rgb = color;
    let con = col.contrast;
    let sh = col.shadows;
    let wh = col.whites;
    let bl = col.blacks;

    if (wh != 0.0) {
        let w_luma = max(get_luma(max(rgb, vec3<f32>(0.0))), 0.0001);
        let w_base = w_luma;
        let t_w = pow(w_base, 0.4545);
        let w_pivot = 0.45;
        let u = max(t_w - w_pivot, 0.0);

        var t_new = t_w;
        if (wh > 0.0) {
            t_new = t_w + wh * 0.25 * (u * u) / (1.0 + u * u);
        } else if (u > 0.0) {
            t_new = w_pivot + u / (1.0 + (-wh) * 0.3 * u);
        }
        let w_ramp = smoothstep(0.25, 0.9, t_w);
        t_new = mix(t_w, t_new, w_ramp);
        rgb *= pow(t_new, 2.2) / w_base;
    }

    let pixel_luma = get_luma(max(rgb, vec3<f32>(0.0)));
    let safe_pixel_luma = max(pixel_luma, 0.0001);

    if (sh != 0.0 || bl != 0.0) {
        let t_pixel = pow(safe_pixel_luma, 0.4545);

        let shadow_lift = sh * t_pixel * pow(max(1.0 - t_pixel, 0.0), 4.5);
        let black_lift = bl * t_pixel * pow(max(1.0 - t_pixel, 0.0), 12.0);
        let lift_amount = max(shadow_lift + black_lift, 0.0);

        let t_pixel_curved = max(t_pixel + shadow_lift + black_lift, 0.0);

        let shadow_pivot = 0.2;
        let stretch_focus = 1.0 - smoothstep(0.1, 0.85, t_pixel);
        let stretch_factor = 1.0 + (lift_amount * stretch_focus * 4.4);
        let contrasted_t = shadow_pivot + (t_pixel_curved - shadow_pivot) * stretch_factor;

        let final_t = max(contrasted_t, 0.0);
        let curved_luma = pow(final_t, 2.2);
        let luma_ratio = curved_luma / safe_pixel_luma;
        rgb *= luma_ratio;

        if (luma_ratio > 1.0) {
            let recovered_luma = get_luma(rgb);
            let boost_amount = clamp((luma_ratio - 1.0) * 0.05, 0.0, 0.12);
            rgb = mix(rgb, vec3<f32>(recovered_luma), boost_amount);
        }
    }

    if (con != 0.0) {
        let safe_rgb = max(rgb, vec3<f32>(0.0));
        let g = 2.2;
        let perceptual = pow(safe_rgb, vec3<f32>(1.0 / g));
        let clamped_perceptual = clamp(perceptual, vec3<f32>(0.0), vec3<f32>(1.0));
        let strength = pow(2.0, con * 1.25);
        let condition = clamped_perceptual < vec3<f32>(0.5);
        let high_part = 1.0 - 0.5 * pow(2.0 * (1.0 - clamped_perceptual), vec3<f32>(strength));
        let low_part = 0.5 * pow(2.0 * clamped_perceptual, vec3<f32>(strength));
        let curved_perceptual = select(high_part, low_part, condition);
        let contrast_adjusted_rgb = pow(curved_perceptual, vec3<f32>(g));
        let mix_factor = smoothstep(vec3<f32>(1.0), vec3<f32>(1.01), safe_rgb);
        rgb = mix(contrast_adjusted_rgb, rgb, mix_factor);
    }
    return rgb;
}

// Highlight compression/boost with a selective pivot; detail restoration is
// identity at zero detail.
fn p_highlights(color_in: vec3<f32>) -> vec3<f32> {
    let highlights_adj = col.highlights;
    if (abs(highlights_adj) < 0.001) {
        return color_in;
    }

    let pixel_luma = get_luma(max(color_in, vec3<f32>(0.0)));
    if (pixel_luma < 0.0001) {
        return color_in;
    }

    const l_pivot: f32 = 0.06;
    if (pixel_luma <= l_pivot) {
        return color_in;
    }

    var target_luma: f32 = pixel_luma;
    if (highlights_adj < 0.0) {
        let k = -highlights_adj;
        let delta_base = max(pixel_luma - l_pivot, 0.0);
        let compression_strength = k * 2.2;
        let compressed_delta = delta_base / (1.0 + compression_strength * (delta_base / (1.0 + delta_base * 0.35)));
        let target_base = l_pivot + compressed_delta;
        let blend = smoothstep(l_pivot, l_pivot + 0.22, pixel_luma);
        target_luma = mix(pixel_luma, target_base, blend);
    } else {
        let boost = highlights_adj * 0.70;
        let delta_base = max(pixel_luma - l_pivot, 0.0);
        let boosted_delta = delta_base * (1.0 + boost / (1.0 + delta_base * 0.25));
        let target_base = l_pivot + boosted_delta;
        let blend = smoothstep(l_pivot, l_pivot + 0.22, pixel_luma);
        target_luma = mix(pixel_luma, target_base, blend);
    }

    let luma_ratio = target_luma / pixel_luma;
    var final_color = color_in * luma_ratio;

    if (highlights_adj < 0.0 && pixel_luma > 1.0) {
        let blowout = smoothstep(1.0, 3.5, pixel_luma) * (-highlights_adj) * 0.40;
        final_color = mix(final_color, vec3<f32>(target_luma), blowout);
    }
    return final_color;
}

fn p_hue_shift(color: vec3<f32>) -> vec3<f32> {
    if (abs(col.hue) < 0.01) {
        return color;
    }
    let srgb_color = linear_to_srgb_extended(max(color, vec3<f32>(0.0)));
    let hsv = rgb_to_hsv(srgb_color);
    var shifted_h = hsv.x + col.hue;
    shifted_h = (shifted_h + 360.0) % 360.0;
    let shifted_srgb = hsv_to_rgb(vec3<f32>(shifted_h, hsv.y, hsv.z));
    return p_srgb_to_linear(shifted_srgb);
}

// Saturation (uniform) and vibrance (weighted toward muted colors, damped on
// skin tones), both luma-preserving.
fn p_creative_color(color: vec3<f32>) -> vec3<f32> {
    let sat = col.saturation;
    let vib = col.vibrance;
    if (sat == 0.0 && vib == 0.0) {
        return color;
    }

    var processed = color;
    let luma = get_luma(max(processed, vec3<f32>(0.0)));

    let srgb = linear_to_srgb_extended(max(processed, vec3<f32>(0.0)));
    let hsv = rgb_to_hsv(srgb);
    let current_sat = hsv.y;
    let hue = hsv.x;

    var vib_factor: f32 = 0.0;
    if (vib != 0.0) {
        if (vib > 0.0) {
            let sat_weight = pow(max(1.0 - current_sat, 0.0), 1.25);
            let skin_center = 25.0;
            let hue_dist = min(abs(hue - skin_center), 360.0 - abs(hue - skin_center));
            let is_skin = 1.0 - smoothstep(12.0, 38.0, hue_dist);
            let skin_dampener = mix(1.0, 0.50, is_skin);
            vib_factor = vib * sat_weight * skin_dampener * 1.5;
        } else {
            let desat_weight = smoothstep(0.05, 0.75, current_sat);
            vib_factor = vib * desat_weight;
        }
    }

    let sat_scale = max(1.0 + sat, 0.0);
    let vib_scale = max(1.0 + vib_factor, 0.0);
    let final_mult = sat_scale * vib_scale;

    processed = mix(vec3<f32>(luma), processed, final_mult);
    return max(processed, vec3<f32>(0.0));
}

// ---------------------------------------------------------------------------
// HSL mixer and color grading, ported from the referenced RapidRAW shader.
// ---------------------------------------------------------------------------

// Hue centres and widths of the eight mixer ranges, matching the reference.
const HSL_CENTERS: array<f32, 8> = array<f32, 8>(
    358.0, 25.0, 60.0, 115.0, 180.0, 225.0, 280.0, 330.0
);
const HSL_WIDTHS: array<f32, 8> = array<f32, 8>(
    35.0, 45.0, 40.0, 90.0, 60.0, 60.0, 55.0, 50.0
);

fn p_raw_hsl_influence(hue: f32, center: f32, width: f32) -> f32 {
    let dist = min(abs(hue - center), 360.0 - abs(hue - center));
    const sharpness: f32 = 1.5;
    let falloff = dist / (width * 0.5);
    return exp(-sharpness * falloff * falloff);
}

// Eight-range HSL mixer: each pixel's hue picks up a normalized blend of the
// range adjustments, with saturation and luminance weighted separately.
fn p_hsl(color: vec3<f32>) -> vec3<f32> {
    var has_adjustments = false;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let entry = col.hsl[i];
        if (abs(entry.hue) > 0.0001
            || abs(entry.saturation) > 0.0001
            || abs(entry.luminance) > 0.0001) {
            has_adjustments = true;
            break;
        }
    }
    if (!has_adjustments) {
        return color;
    }

    let safe_color = max(color, vec3<f32>(0.0));
    if (abs(safe_color.r - safe_color.g) < 0.001 && abs(safe_color.g - safe_color.b) < 0.001) {
        return safe_color;
    }
    let original_hsv = rgb_to_hsv(linear_to_srgb_extended(safe_color));
    let original_luma = get_luma(safe_color);

    let saturation_mask = smoothstep(0.05, 0.20, original_hsv.y);
    let luminance_weight = smoothstep(0.0, 1.0, original_hsv.y);
    if (saturation_mask < 0.001 && luminance_weight < 0.001) {
        return safe_color;
    }

    let original_hue = original_hsv.x;
    var raw_influences: array<f32, 8>;
    var total_raw_influence: f32 = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let influence = p_raw_hsl_influence(original_hue, HSL_CENTERS[i], HSL_WIDTHS[i]);
        raw_influences[i] = influence;
        total_raw_influence += influence;
    }

    var total_hue_shift: f32 = 0.0;
    var total_sat_multiplier: f32 = 0.0;
    var total_lum_adjust: f32 = 0.0;
    for (var i = 0u; i < 8u; i = i + 1u) {
        let normalized_influence = raw_influences[i] / total_raw_influence;
        let entry = col.hsl[i];
        total_hue_shift += entry.hue * 2.0 * normalized_influence * saturation_mask;
        total_sat_multiplier += entry.saturation * normalized_influence * saturation_mask;
        total_lum_adjust += entry.luminance * normalized_influence * luminance_weight;
    }

    if (original_hsv.y * (1.0 + total_sat_multiplier) < 0.0001) {
        let final_luma = original_luma * (1.0 + total_lum_adjust);
        return vec3<f32>(final_luma);
    }
    var hsv = original_hsv;
    hsv.x = (hsv.x + total_hue_shift + 360.0) % 360.0;
    hsv.y = clamp(hsv.y * (1.0 + total_sat_multiplier), 0.0, 1.0);
    let hs_shifted_rgb = p_srgb_to_linear(hsv_to_rgb(vec3<f32>(hsv.x, hsv.y, original_hsv.z)));
    let new_luma = get_luma(hs_shifted_rgb);
    let target_luma = original_luma * (1.0 + total_lum_adjust);
    if (new_luma < 0.0001) {
        return vec3<f32>(max(0.0, target_luma));
    }
    return hs_shifted_rgb * (target_luma / new_luma);
}

// Four-way split toning: shadow/midtone/highlight/global wheels tint and lift
// luminance through smooth crossovers shaped by blending and balance.
fn p_color_grading(color: vec3<f32>) -> vec3<f32> {
    let luma = get_luma(max(vec3<f32>(0.0), color));
    let base_shadow_crossover = 0.1;
    let base_highlight_crossover = 0.5;
    let balance_range = 0.5;
    let balance = col.grading_balance;
    let shadow_crossover = base_shadow_crossover + max(0.0, -balance) * balance_range;
    let highlight_crossover = base_highlight_crossover - max(0.0, balance) * balance_range;
    let feather = 0.2 * col.grading_blending;
    let final_shadow_crossover = min(shadow_crossover, highlight_crossover - 0.01);
    let shadow_mask = 1.0 - smoothstep(final_shadow_crossover - feather, final_shadow_crossover + feather, luma);
    let highlight_mask = smoothstep(highlight_crossover - feather, highlight_crossover + feather, luma);
    let midtone_mask = max(0.0, 1.0 - shadow_mask - highlight_mask);

    let shadow = col.color_grading[0];
    let midtone = col.color_grading[1];
    let highlight = col.color_grading[2];
    let global = col.color_grading[3];

    var graded = color;
    if (shadow.saturation > 0.001) {
        let tint = hsv_to_rgb(vec3<f32>(shadow.hue, 1.0, 1.0));
        graded += (tint - 0.5) * shadow.saturation * shadow_mask * 0.1;
    }
    graded += shadow.luminance * shadow_mask * 0.5;
    if (midtone.saturation > 0.001) {
        let tint = hsv_to_rgb(vec3<f32>(midtone.hue, 1.0, 1.0));
        graded += (tint - 0.5) * midtone.saturation * midtone_mask * 0.6;
    }
    graded += midtone.luminance * midtone_mask * 0.8;
    if (highlight.saturation > 0.001) {
        let tint = hsv_to_rgb(vec3<f32>(highlight.hue, 1.0, 1.0));
        graded += (tint - 0.5) * highlight.saturation * highlight_mask * 0.8;
    }
    graded += highlight.luminance * highlight_mask * 1.0;
    if (global.saturation > 0.001) {
        let tint = hsv_to_rgb(vec3<f32>(global.hue, 1.0, 1.0));
        graded += (tint - 0.5) * global.saturation * 1.0;
    }
    graded += global.luminance * 1.0;
    return graded;
}

// Filmic brightness on the sRGB-encoded tone: a rational curve on luma with
// chroma scaling in tow, ported from the reference.
fn p_filmic_exposure(color_in: vec3<f32>) -> vec3<f32> {
    let brightness_adj = col.brightness;
    if (brightness_adj == 0.0) {
        return color_in;
    }
    const MIDTONE_STRENGTH: f32 = 1.2;
    const TOP_ANCHOR: f32 = 1.0;

    let original_luma = get_luma(color_in);
    if (abs(original_luma) < 0.00001) {
        return color_in;
    }

    let rational_adj = brightness_adj * RATIONAL_CURVE_MIX;
    let k = pow(2.0, -rational_adj * MIDTONE_STRENGTH);
    let luma_abs = abs(original_luma);

    var shaped_luma_abs: f32;
    if (luma_abs <= TOP_ANCHOR) {
        let luma_norm = luma_abs / TOP_ANCHOR;
        let shaped_norm = luma_norm / (luma_norm + (1.0 - luma_norm) * k);
        shaped_luma_abs = shaped_norm * TOP_ANCHOR;
    } else {
        shaped_luma_abs = TOP_ANCHOR + (luma_abs - TOP_ANCHOR) * k;
    }

    let new_luma_abs = shaped_luma_abs;
    let new_luma = sign(original_luma) * new_luma_abs;
    let chroma = color_in - vec3<f32>(original_luma);

    let total_luma_scale = new_luma_abs / luma_abs;
    let luma_weight = clamp(new_luma_abs, 0.0, 2.0) * 0.5;
    let dynamic_exp = mix(0.95, 0.65, luma_weight);
    let base_chroma_scale = pow(total_luma_scale, dynamic_exp);

    let highlight_rolloff = clamp((1.05 - new_luma_abs) / max(1.05 - original_luma, 1e-4), 0.0, 1.0);
    let chroma_scale = base_chroma_scale * mix(1.0, highlight_rolloff, clamp(abs(brightness_adj), 0.0, 1.0));

    return vec3<f32>(new_luma) + chroma * chroma_scale;
}

const RATIONAL_CURVE_MIX: f32 = 1.0;
