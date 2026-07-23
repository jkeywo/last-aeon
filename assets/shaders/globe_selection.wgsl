// A StandardMaterial extension for the strategic globes. Province IDs are
// fetched by integer texel coordinate, deliberately bypassing filtering so a
// selection cannot bleed over a border.

#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    mesh_view_bindings::globals,
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, main_pass_post_lighting_processing},
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> selection: vec4<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var province_ids: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var province_ids_sampler: sampler;

fn srgb_to_linear(rgb: vec3<f32>) -> vec3<f32> {
    let low = rgb / vec3<f32>(12.92);
    let high = pow((rgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(low, high, rgb > vec3<f32>(0.04045));
}

@fragment
fn fragment(vertex_output: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(vertex_output, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    let dimensions = textureDimensions(province_ids);
    let u = fract(vertex_output.uv.x);
    let x = min(i32(floor(u * f32(dimensions.x))), i32(dimensions.x) - 1);
    let y = clamp(
        i32(floor(vertex_output.uv.y * f32(dimensions.y))),
        0,
        i32(dimensions.y) - 1,
    );
    let province_id = textureLoad(province_ids, vec2<i32>(x, y), 0).rgb;
    let selected_linear = srgb_to_linear(selection.rgb);
    let matches_selection = selection.a > 0.5 && all(abs(province_id - selected_linear) < vec3<f32>(0.00001));

    // A two-second pulse marking the selected territory. With no pin sphere
    // to point at it, this brightening is the only cue, so it runs strong.
    let pulse = 0.35 + 0.15 * (0.5 + 0.5 * sin(globals.time * 3.14159265));
    if (matches_selection) {
        let boosted = pbr_input.material.base_color.rgb * (1.0 + pulse);
        pbr_input.material.base_color = vec4<f32>(boosted, pbr_input.material.base_color.a);
    }

    var out: FragmentOutput;
    out.color = pbr_input.material.base_color;
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
