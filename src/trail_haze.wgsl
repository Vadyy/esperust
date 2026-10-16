#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::{view, view_transmission_texture, view_transmission_sampler}

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var gradient_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var gradient_sampler: sampler;

fn luminance(uv: vec2<f32>) -> f32 {
    return dot(textureSample(gradient_texture, gradient_sampler, uv).rgb,
        vec3<f32>(0.2125, 0.7154, 0.0721));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let left = luminance(in.uv - vec2<f32>(0.01, 0.0));
    let right = luminance(in.uv + vec2<f32>(0.01, 0.0));
    let up = luminance(in.uv - vec2<f32>(0.0, 0.01));
    let down = luminance(in.uv + vec2<f32>(0.0, 0.01));
    let normal = normalize(vec3<f32>(left - right, up - down, 0.05));
    let clip = view.clip_from_world * in.world_position;
    let projected = vec4<f32>(clip.x * 0.5 + clip.w * 0.5,
        -clip.y * 0.5 + clip.w * 0.5, clip.z * 0.5 + clip.w * 0.5, clip.w);
    let refracted = vec4<f32>(projected.x + normal.x * 0.00015,
        projected.y + normal.z * 0.00015, projected.z, projected.w + normal.y * 0.35);
    return textureSampleLevel(view_transmission_texture, view_transmission_sampler,
        refracted.xy / refracted.w, 0.0);
}
