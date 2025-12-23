#![expect(missing_docs, reason = "Not all docs are written yet, see #3492.")]
#![expect(unsafe_code, reason = "Unsafe code is used to improve performance.")]
#![cfg_attr(
    any(docsrs, docsrs_dep),
    expect(
        internal_features,
        reason = "rustdoc_internals is needed for fake_variadic"
    )
)]
#![cfg_attr(any(docsrs, docsrs_dep), feature(doc_cfg, rustdoc_internals))]

// Copyright (c) 2019-2024 Bevy Contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//
// This code is vendored from Bevy: https://github.com/bevyengine/bevy
// Original repository: https://github.com/bevyengine/bevy
// Vendored from commit: 566358363126dd69f6e457e47f306c68f8041d2a (v0.17.2)
// Adapted for Marathon engine.
//
// This module contains vendored code from:
// - bevy_render 0.17.2 (core rendering)
// - bevy_core_pipeline 0.17.2 (render pipelines)
// - bevy_pbr 0.17.2 (materials and lighting)
//
// External dependencies (NOT vendored):
// - bevy_ecs, bevy_app, bevy_asset, bevy_transform, bevy_window, etc.

// Re-export macro from resource_macros
pub use crate::define_atomic_id;

// Re-export derive macros from macros
pub use macros::{AsBindGroup, RenderLabel, RenderSubGraph};

#[cfg(target_pointer_width = "16")]
compile_error!("bevy_render cannot compile for a 16-bit platform.");

// ============================================================================
// bevy_render modules
// ============================================================================
pub mod alpha;
pub mod batching;
pub mod camera;
pub mod diagnostic;
pub mod erased_render_asset;
pub mod experimental;
pub mod extract_component;
pub mod extract_instances;
mod extract_param;
pub mod extract_resource;
pub mod globals;
pub mod gpu_component_array_buffer;
pub mod gpu_readback;
pub mod mesh;
#[cfg(not(target_arch = "wasm32"))]
pub mod pipelined_rendering;
pub mod render_asset;
pub mod render_graph;
pub mod render_phase;
pub mod render_resource;
pub mod renderer;
pub mod settings;
pub mod storage;
pub mod sync_component;
pub mod sync_world;
pub mod texture;
pub mod view;

// ============================================================================
// bevy_core_pipeline modules
// ============================================================================
pub mod blit;
pub mod core_2d;
pub mod core_3d;
pub mod deferred;
pub mod oit;
pub mod prepass;
pub mod tonemapping;
pub mod upscaling;
pub mod skybox;

pub use skybox::Skybox;

mod fullscreen_vertex_shader;
pub use fullscreen_vertex_shader::FullscreenShader;

// ============================================================================
// bevy_pbr module
// ============================================================================
pub mod pbr;

// Re-export commonly used types from pbr for convenience
pub use pbr::StandardMaterial;
// These light and shadow types come from bevy_light
pub use bevy_light::{
    AmbientLight, DirectionalLight, DirectionalLightShadowMap,
    NotShadowCaster, NotShadowReceiver, PointLight, PointLightShadowMap,
    SpotLight, TransmittedShadowReceiver,
};

// ============================================================================
// Re-exports from bevy_render for convenience
// ============================================================================
pub use alpha::AlphaMode;
pub use camera::CameraRenderGraph;
// These camera types come from bevy_camera, not vendored code
pub use bevy_camera::{Camera, Camera2d, Camera3d, OrthographicProjection, PerspectiveProjection, Projection, ScalingMode};
pub use extract_component::{ExtractComponent, ExtractComponentPlugin};
pub use extract_resource::{ExtractResource, ExtractResourcePlugin};
pub use bevy_mesh::{Mesh3d, Meshable};
// MeshMaterial3d is from pbr module
pub use pbr::MeshMaterial3d;
pub use render_asset::{RenderAssetPlugin, prepare_assets};
// These come from bevy_asset
pub use bevy_asset::RenderAssetUsages;
pub use render_graph::RenderGraph;
pub use render_phase::{
    BinnedRenderPhase, CachedRenderPipelinePhaseItem, DrawFunctions, PhaseItem, RenderCommand,
    RenderCommandState, SortedRenderPhase, TrackedRenderPass,
};
pub use render_resource::{
    BindGroup, BindGroupEntries, BindGroupLayout, Buffer, BufferUsages, BufferVec,
    ComputePipeline, PipelineCache, RenderPipeline, Sampler,
    Texture, TextureFormat, TextureUsages,
};
// These shader types come from bevy_shader
pub use bevy_shader::{ShaderDefVal, ShaderRef};
pub use renderer::{RenderAdapter, RenderAdapterInfo, RenderDevice, RenderQueue};
pub use settings::{RenderCreation, WgpuSettings};
pub use texture::GpuImage;
// These come from bevy_image
pub use bevy_image::{BevyDefault, Image, ImageFormat, ImageSampler, TextureFormatPixelInfo};
pub use view::{
    ColorGrading, ExtractedView, Msaa, ViewTarget,
};
pub use bevy_camera::Exposure;
// These come from bevy_camera
pub use bevy_camera::visibility::{RenderLayers, VisibleEntities};
// Tonemapping comes from the vendored tonemapping module
pub use tonemapping::Tonemapping;

// ============================================================================
// Prelude module (from bevy_render)
// ============================================================================
pub mod prelude {
    #[doc(hidden)]
    pub use crate::render::{
        alpha::AlphaMode,
        camera::NormalizedRenderTargetExt as _,
        texture::ManualTextureViews,
        view::Msaa,
        ExtractSchedule,
    };
}

// ============================================================================
// Additional re-exports from bevy_render
// ============================================================================
pub use extract_param::Extract;
pub use sync_world::{RenderEntity, SyncToRenderWorld};

// Re-export main plugin types
// Note: RenderPlugin is defined in bevy_render's lib.rs and wasn't vendored
// MainWorld is defined above in this file

// Re-export schedule types
#[cfg(not(target_arch = "wasm32"))]
pub use pipelined_rendering::PipelinedRenderingPlugin;

// Re-export RenderSystems and other core types
use bevy_ecs::schedule::{ScheduleLabel, SystemSet};
use bitflags::bitflags;

/// The systems sets of the default rendering schedule.
#[derive(Debug, Hash, PartialEq, Eq, Clone, SystemSet)]
pub enum RenderSystems {
    ExtractCommands,
    PrepareAssets,
    PrepareMeshes,
    ManageViews,
    Queue,
    QueueMeshes,
    QueueSweep,
    PhaseSort,
    Prepare,
    PrepareResources,
    PrepareResourcesCollectPhaseBuffers,
    PrepareResourcesFlush,
    PrepareBindGroups,
    Render,
    Cleanup,
    PostCleanup,
}

bitflags! {
    /// Debugging flags that can optionally be set when constructing the renderer.
    #[derive(Clone, Copy, PartialEq, Default, Debug)]
    pub struct RenderDebugFlags: u8 {
        const ALLOW_COPIES_FROM_INDIRECT_PARAMETERS = 1;
    }
}

/// The startup schedule of the RenderApp
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone, Default)]
pub struct ExtractSchedule;

/// The startup schedule of the [`RenderApp`]
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone, Default)]
pub struct RenderStartup;

/// The main render schedule.
#[derive(ScheduleLabel, Debug, Hash, PartialEq, Eq, Clone, Default)]
pub struct Render;

/// A label for the rendering sub-app.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, bevy_app::AppLabel)]
pub struct RenderApp;

use bevy_ecs::world::World;
use bevy_ecs::resource::Resource;
use core::ops::{Deref, DerefMut};

/// See [`Extract`] for more details.
#[derive(Resource, Default)]
pub struct MainWorld(World);

impl Deref for MainWorld {
    type Target = World;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MainWorld {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// Graph module for camera driver label
pub mod graph {
    use crate::render::render_graph::RenderLabel;

    #[derive(Debug, Hash, PartialEq, Eq, Clone, RenderLabel)]
    pub struct CameraDriverLabel;
}

/// Get the Adreno GPU model number if the adapter is an Adreno GPU.
pub fn get_adreno_model(adapter_info: &renderer::RenderAdapterInfo) -> Option<u32> {
    if !cfg!(target_os = "android") {
        return None;
    }

    let adreno_model = adapter_info.name.strip_prefix("Adreno (TM) ")?;

    // Take suffixes into account (like Adreno 642L).
    Some(
        adreno_model
            .chars()
            .map_while(|c| c.to_digit(10))
            .fold(0, |acc, digit| acc * 10 + digit),
    )
}

/// Get the Mali driver version if the adapter is a Mali GPU.
pub fn get_mali_driver_version(adapter_info: &renderer::RenderAdapterInfo) -> Option<u32> {
    if !cfg!(target_os = "android") {
        return None;
    }

    if !adapter_info.name.contains("Mali") {
        return None;
    }
    let driver_info = &adapter_info.driver_info;
    if let Some(start_pos) = driver_info.find("v1.r")
        && let Some(end_pos) = driver_info[start_pos..].find('p')
    {
        let start_idx = start_pos + 4; // Skip "v1.r"
        let end_idx = start_pos + end_pos;
        driver_info[start_idx..end_idx].parse().ok()
    } else {
        None
    }
}

// ============================================================================
// bevy_core_pipeline plugin and re-exports
// ============================================================================

use bevy_app::{App, Plugin};
use bevy_asset::embedded_asset;

#[derive(Default)]
pub struct CorePipelinePlugin;

impl Plugin for CorePipelinePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "fullscreen_vertex_shader/fullscreen.wgsl");

        app.add_plugins((core_2d::Core2dPlugin, core_3d::Core3dPlugin, deferred::copy_lighting_id::CopyDeferredLightingIdPlugin))
            .add_plugins((
                blit::BlitPlugin,
                tonemapping::TonemappingPlugin,
                upscaling::UpscalingPlugin,
                oit::OrderIndependentTransparencyPlugin,
                experimental::mip_generation::MipGenerationPlugin,
            ));

        #[cfg(not(target_arch = "wasm32"))]
        {
            let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
                return;
            };
            render_app.init_resource::<FullscreenShader>();
        }
    }
}
