#import bevy_pbr::forward_io::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var base_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var base_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var lightmap_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var lightmap_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> alpha_settings: vec4<f32>;

fn source_to_linear(color: vec3<f32>) -> vec3<f32> {
    let value = clamp(color, vec3<f32>(0.0), vec3<f32>(1.0));
    return select(pow((value + 0.055) / 1.055, vec3<f32>(2.4)), value / 12.92, value <= vec3<f32>(0.04045));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let base = textureSample(base_texture, base_sampler, in.uv);
    let light = textureSample(lightmap_texture, lightmap_sampler, in.uv_b);
    let alpha = base.a * alpha_settings.x;
    if alpha < alpha_settings.y { discard; }
    return vec4<f32>(source_to_linear(base.rgb * light.rgb * 2.0), alpha);
}
