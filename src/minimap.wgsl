#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(1) @binding(0) var map_texture: texture_2d<f32>;
@group(1) @binding(1) var map_sampler: sampler;
@group(1) @binding(2) var mask_texture: texture_2d<f32>;
@group(1) @binding(3) var mask_sampler: sampler;
@group(1) @binding(4) var<uniform> region: vec4<f32>;
@group(1) @binding(5) var<uniform> rotation: vec4<f32>;

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let q = in.uv * 2.0 - 1.0;
    let offset = vec2<f32>(
        rotation.x * q.x - rotation.y * q.y,
        rotation.y * q.x + rotation.x * q.y,
    );
    let uv = region.xy + offset * region.zw;
    let map = textureSample(map_texture, map_sampler, uv);
    let mask = textureSample(mask_texture, mask_sampler, in.uv).a;
    return vec4<f32>(map.rgb, map.a * mask);
}
