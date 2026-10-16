use bevy::{
    asset::{embedded_asset, embedded_path, AssetPath},
    core_pipeline::core_3d::graph::{Core3d, Node3d},
    mesh::MeshVertexBufferLayoutRef,
    pbr::{MaterialPipeline, MaterialPipelineKey},
    prelude::*,
    render::render_resource::SpecializedMeshPipelineError,
    render::render_resource::{AsBindGroup, Face, RenderPipelineDescriptor},
    render::{RenderApp, render_graph::RenderGraph},
    shader::ShaderRef,
};

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct DualTexture {
    #[texture(0)]
    #[sampler(1)]
    pub base: Handle<Image>,
    #[texture(2)]
    #[sampler(3)]
    pub lightmap: Handle<Image>,
    #[uniform(4)]
    pub alpha_settings: Vec4,
    pub alpha_mode: AlphaMode,
}
impl Material for DualTexture {
    fn fragment_shader() -> ShaderRef {
        AssetPath::from_path_buf(embedded_path!("dual_texture.wgsl"))
            .with_source("embedded")
            .into()
    }
    fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = Some(Face::Back);
        Ok(())
    }
}
fn register_shaders(app: &mut App) {
    embedded_asset!(app, "dual_texture.wgsl");
    embedded_asset!(app, "trail_haze.wgsl");
}

pub struct DualTexturePlugin;
impl Plugin for DualTexturePlugin {
    fn build(&self, app: &mut App) {
        register_shaders(app);
        app.add_plugins((
            MaterialPlugin::<DualTexture>::default(),
            MaterialPlugin::<TrailHaze>::default(),
        ));
        let mut graph = app
            .sub_app_mut(RenderApp)
            .world_mut()
            .resource_mut::<RenderGraph>();
        haze_after_transparency(
            graph
                .get_sub_graph_mut(Core3d)
                .expect("Core3d render graph"),
        );
    }
}

fn haze_after_transparency(graph: &mut RenderGraph) {
    graph
        .remove_node_edge(Node3d::MainOpaquePass, Node3d::MainTransmissivePass)
        .unwrap();
    graph
        .remove_node_edge(Node3d::MainTransmissivePass, Node3d::MainTransparentPass)
        .unwrap();
    graph
        .remove_node_edge(Node3d::MainTransparentPass, Node3d::EndMainPass)
        .unwrap();
    graph.add_node_edges((
        Node3d::MainOpaquePass,
        Node3d::MainTransparentPass,
        Node3d::MainTransmissivePass,
        Node3d::EndMainPass,
    ));
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct TrailHaze {
    #[texture(0)]
    #[sampler(1)]
    pub gradient: Handle<Image>,
}

impl Material for TrailHaze {
    fn fragment_shader() -> ShaderRef {
        AssetPath::from_path_buf(embedded_path!("trail_haze.wgsl"))
            .with_source("embedded")
            .into()
    }
    fn reads_view_transmission_texture(&self) -> bool {
        true
    }
    fn enable_prepass() -> bool {
        false
    }
    fn enable_shadows() -> bool {
        false
    }
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        _key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        if let Some(depth) = descriptor.depth_stencil.as_mut() {
            depth.depth_write_enabled = false;
        }
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = None;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::render_graph::{Edge, EmptyNode, RenderLabel};

    #[test]
    fn material_shader_paths_resolve_to_the_registered_embedded_assets() {
        use bevy::asset::io::{embedded::EmbeddedAssetRegistry, AssetSourceId};
        let mut app = App::new();
        app.init_resource::<EmbeddedAssetRegistry>();
        register_shaders(&mut app);
        let registry = app.world().resource::<EmbeddedAssetRegistry>();
        for shader in [DualTexture::fragment_shader(), TrailHaze::fragment_shader()] {
            let ShaderRef::Path(path) = shader else { panic!("Expected an embedded shader path") };
            assert_eq!(path.source(), &AssetSourceId::from("embedded"));
            assert!(registry.remove_asset(path.path()).is_some(), "Unregistered shader: {path}");
        }
    }

    #[test]
    fn haze_scene_copy_occurs_after_the_warm_transparent_ribbon() {
        let mut graph = RenderGraph::default();
        for node in [
            Node3d::MainOpaquePass,
            Node3d::MainTransmissivePass,
            Node3d::MainTransparentPass,
            Node3d::EndMainPass,
        ] {
            graph.add_node(node, EmptyNode);
        }
        graph.add_node_edges((
            Node3d::MainOpaquePass,
            Node3d::MainTransmissivePass,
            Node3d::MainTransparentPass,
            Node3d::EndMainPass,
        ));
        haze_after_transparency(&mut graph);
        for (from, to) in [
            (Node3d::MainOpaquePass, Node3d::MainTransparentPass),
            (Node3d::MainTransparentPass, Node3d::MainTransmissivePass),
            (Node3d::MainTransmissivePass, Node3d::EndMainPass),
        ] {
            assert!(graph.has_edge(&Edge::NodeEdge {
                output_node: from.intern(),
                input_node: to.intern()
            }));
        }
        assert!(!graph.has_edge(&Edge::NodeEdge {
            output_node: Node3d::MainTransmissivePass.intern(),
            input_node: Node3d::MainTransparentPass.intern()
        }));
    }
}
