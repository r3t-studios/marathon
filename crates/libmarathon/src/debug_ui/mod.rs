// Copyright (c) 2021 Vladyslav Batyrenko
// SPDX-License-Identifier: MIT
//
// This code is vendored from bevy_egui: https://github.com/vladbat00/bevy_egui
// Original author: Vladyslav Batyrenko <vladyslav.batyrenko@gmail.com>
// Adapted for Marathon engine with simplified feature set (desktop-only, single window).

#![allow(clippy::type_complexity)]

//! Debug UI integration using egui for the Marathon engine.
//!
//! This is a vendored and simplified version of bevy_egui, stripped down to support:
//! - Desktop platforms only (no WASM/web)
//! - Single window
//! - No picking/accessibility features
//! - Works with Marathon's custom executor

/// Helpers for converting Bevy types into Egui ones and vice versa.
pub mod helpers;
/// Systems for translating Bevy input messages into Egui input.
pub mod input;
/// Systems for handling Egui output.
pub mod output;
/// Rendering Egui with [`bevy_render`].
pub mod render;

pub use egui;

use self::input::*;
use bevy::app::prelude::*;
use bevy::asset::{AssetEvent, AssetId, Assets, Handle, load_internal_asset};
use bevy::prelude::{Deref, DerefMut, Shader};
use bevy::ecs::{
    prelude::*,
    query::{QueryData, QuerySingleError},
    schedule::{InternedScheduleLabel, ScheduleLabel},
    system::SystemParam,
};
use bevy::image::{Image, ImageSampler};
use bevy::input::InputSystems;
#[allow(unused_imports)]
use bevy::log;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::reflect::Reflect;
use bevy::render::{
    ExtractSchedule, Render, RenderApp, RenderSystems,
    extract_resource::{ExtractResource, ExtractResourcePlugin},
    render_resource::SpecializedRenderPipelines,
};
use bevy::window::CursorIcon;
use output::process_output_system;

/// Adds all Egui resources and render graph nodes.
pub struct EguiPlugin {
    /// The bindless mode array size for egui rendering.
    pub bindless_mode_array_size: Option<std::num::NonZero<u32>>,
}

impl Default for EguiPlugin {
    fn default() -> Self {
        Self {
            bindless_mode_array_size: None,
        }
    }
}

/// A resource for storing global plugin settings.
#[derive(Clone, Debug, Resource, Reflect)]
pub struct EguiGlobalSettings {
    /// Set this to `false` if you want to control the creation of [`EguiContext`] instances manually.
    pub auto_create_primary_context: bool,
    /// Controls running of the input systems.
    pub input_system_settings: EguiInputSystemSettings,
    /// Controls whether `bevy_egui` updates [`CursorIcon`], enabled by default.
    pub enable_cursor_icon_updates: bool,
    /// Controls whether focused non-window contexts can be updated (disabled for simplicity).
    #[reflect(ignore)]
    pub enable_focused_non_window_context_updates: bool,
}

impl Default for EguiGlobalSettings {
    fn default() -> Self {
        Self {
            auto_create_primary_context: true,
            input_system_settings: EguiInputSystemSettings::default(),
            enable_cursor_icon_updates: true,
            enable_focused_non_window_context_updates: false,
        }
    }
}

/// A component for storing Egui context settings.
#[derive(Clone, Debug, Component, Reflect)]
pub struct EguiContextSettings {
    /// If set to `true`, a user is expected to call [`egui::Context::run`] manually.
    pub run_manually: bool,
    /// Global scale factor for Egui widgets (`1.0` by default).
    pub scale_factor: f32,
    /// Controls running of the input systems.
    pub input_system_settings: EguiInputSystemSettings,
    /// Controls whether updates [`CursorIcon`], enabled by default.
    pub enable_cursor_icon_updates: bool,
    /// Controls whether IME (Input Method Editor) is enabled (disabled for simplicity).
    #[reflect(ignore)]
    pub enable_ime: bool,
}

impl Default for EguiContextSettings {
    fn default() -> Self {
        Self {
            run_manually: false,
            scale_factor: 1.0,
            input_system_settings: EguiInputSystemSettings::default(),
            enable_cursor_icon_updates: true,
            enable_ime: false,
        }
    }
}

impl PartialEq for EguiContextSettings {
    fn eq(&self, other: &Self) -> bool {
        self.scale_factor == other.scale_factor
    }
}

#[derive(Clone, Debug, Reflect, PartialEq, Eq)]
/// All the systems are enabled by default.
pub struct EguiInputSystemSettings {
    /// Controls running of the [`write_modifiers_keys_state_system`] system.
    pub run_write_modifiers_keys_state_system: bool,
    /// Controls running of the [`write_window_pointer_moved_messages_system`] system.
    pub run_write_window_pointer_moved_messages_system: bool,
    /// Controls running of the [`write_pointer_button_messages_system`] system.
    pub run_write_pointer_button_messages_system: bool,
    /// Controls running of the [`write_window_touch_messages_system`] system.
    pub run_write_window_touch_messages_system: bool,
    /// Controls running of the [`write_mouse_wheel_messages_system`] system.
    pub run_write_mouse_wheel_messages_system: bool,
    /// Controls running of the [`write_keyboard_input_messages_system`] system.
    pub run_write_keyboard_input_messages_system: bool,
    /// Disabled for simplicity (non-window contexts)
    #[reflect(ignore)]
    pub run_write_non_window_pointer_moved_messages_system: bool,
    /// Disabled for simplicity (non-window contexts)
    #[reflect(ignore)]
    pub run_write_non_window_touch_messages_system: bool,
    /// Disabled for simplicity (IME)
    #[reflect(ignore)]
    pub run_write_ime_messages_system: bool,
    /// Disabled for simplicity (file drag and drop)
    #[reflect(ignore)]
    pub run_write_file_dnd_messages_system: bool,
}

impl Default for EguiInputSystemSettings {
    fn default() -> Self {
        Self {
            run_write_modifiers_keys_state_system: true,
            run_write_window_pointer_moved_messages_system: true,
            run_write_pointer_button_messages_system: true,
            run_write_window_touch_messages_system: true,
            run_write_mouse_wheel_messages_system: true,
            run_write_keyboard_input_messages_system: true,
            run_write_non_window_pointer_moved_messages_system: false,
            run_write_non_window_touch_messages_system: false,
            run_write_ime_messages_system: false,
            run_write_file_dnd_messages_system: false,
        }
    }
}

/// Use this schedule to run your UI systems with the primary Egui context.
#[derive(ScheduleLabel, Clone, Debug, PartialEq, Eq, Hash)]
pub struct EguiPrimaryContextPass;

/// A marker component for a primary Egui context.
#[derive(Component, Clone)]
#[require(EguiMultipassSchedule::new(EguiPrimaryContextPass))]
pub struct PrimaryEguiContext;

/// Add this component to your Egui context to enable multi-pass support.
#[derive(Component, Clone)]
#[require(EguiContext)]
pub struct EguiMultipassSchedule(pub InternedScheduleLabel);

impl EguiMultipassSchedule {
    /// Constructs the component from a schedule label.
    pub fn new(schedule: impl ScheduleLabel) -> Self {
        Self(schedule.intern())
    }
}

/// Is used for storing Egui context input.
#[derive(Component, Clone, Debug, Default, Deref, DerefMut)]
pub struct EguiInput(pub egui::RawInput);

/// Intermediate output buffer generated on an Egui pass end.
#[derive(Component, Clone, Default, Deref, DerefMut)]
pub struct EguiFullOutput(pub Option<egui::FullOutput>);

/// Is used for storing Egui shapes and textures delta.
#[derive(Component, Clone, Default, Debug)]
pub struct EguiRenderOutput {
    /// Pairs of rectangles and paint commands.
    pub paint_jobs: Vec<egui::ClippedPrimitive>,
    /// The change in egui textures since last frame.
    pub textures_delta: egui::TexturesDelta,
}

impl EguiRenderOutput {
    /// Returns `true` if the output has no Egui shapes and no textures delta.
    pub fn is_empty(&self) -> bool {
        self.paint_jobs.is_empty() && self.textures_delta.is_empty()
    }
}

/// Stores last Egui output.
#[derive(Component, Clone, Default)]
pub struct EguiOutput {
    /// The field gets updated during [`process_output_system`].
    pub platform_output: egui::PlatformOutput,
}

/// A component for storing `bevy_egui` context.
#[derive(Clone, Component, Default)]
#[require(
    EguiContextSettings,
    EguiInput,
    EguiContextPointerPosition,
    EguiContextPointerTouchId,
    EguiFullOutput,
    EguiRenderOutput,
    EguiOutput,
    CursorIcon
)]
pub struct EguiContext {
    ctx: egui::Context,
}

impl EguiContext {
    /// Borrows the underlying Egui context mutably.
    #[must_use]
    pub fn get_mut(&mut self) -> &mut egui::Context {
        &mut self.ctx
    }

    /// Borrows the underlying Egui context immutably.
    #[must_use]
    pub fn get(&self) -> &egui::Context {
        &self.ctx
    }
}

type EguiContextsQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut EguiContext,
        Option<&'static PrimaryEguiContext>,
    ),
>;

#[derive(SystemParam)]
/// A helper SystemParam that provides a way to get [`EguiContext`] with less boilerplate.
pub struct EguiContexts<'w, 's> {
    q: EguiContextsQuery<'w, 's>,
    user_textures: ResMut<'w, EguiUserTextures>,
}

impl EguiContexts<'_, '_> {
    /// Returns an Egui context with the [`PrimaryEguiContext`] component.
    #[inline]
    pub fn ctx_mut(&mut self) -> Result<&mut egui::Context, QuerySingleError> {
        self.q.iter_mut().fold(
            Err(QuerySingleError::NoEntities("".into())),
            |result, (ctx, primary)| match (&result, primary) {
                (Err(QuerySingleError::MultipleEntities(_)), _) => result,
                (Err(QuerySingleError::NoEntities(_)), Some(_)) => Ok(ctx.into_inner().get_mut()),
                (Err(QuerySingleError::NoEntities(_)), None) => result,
                (Ok(_), Some(_)) => Err(QuerySingleError::MultipleEntities("".into())),
                (Ok(_), None) => result,
            },
        )
    }

    /// Can accept either a strong or a weak handle.
    pub fn add_image(&mut self, image: EguiTextureHandle) -> egui::TextureId {
        self.user_textures.add_image(image)
    }

    /// Removes the image handle and an Egui texture id associated with it.
    #[track_caller]
    pub fn remove_image(&mut self, image: impl Into<AssetId<Image>>) -> Option<egui::TextureId> {
        self.user_textures.remove_image(image)
    }

    /// Returns an associated Egui texture id.
    #[must_use]
    #[track_caller]
    pub fn image_id(&self, image: impl Into<AssetId<Image>>) -> Option<egui::TextureId> {
        self.user_textures.image_id(image)
    }
}

/// A resource for storing user textures.
#[derive(Clone, Resource, ExtractResource)]
pub struct EguiUserTextures {
    textures: HashMap<AssetId<Image>, (EguiTextureHandle, u64)>,
    free_list: Vec<u64>,
}

impl Default for EguiUserTextures {
    fn default() -> Self {
        Self {
            textures: HashMap::default(),
            free_list: vec![0],
        }
    }
}

impl EguiUserTextures {
    /// Adds an image and returns its texture ID.
    pub fn add_image(&mut self, image: EguiTextureHandle) -> egui::TextureId {
        let (_, id) = *self.textures.entry(image.asset_id()).or_insert_with(|| {
            let id = self
                .free_list
                .pop()
                .expect("free list must contain at least 1 element");
            log::debug!("Add a new image (id: {}, handle: {:?})", id, image);
            if self.free_list.is_empty() {
                self.free_list.push(id.checked_add(1).expect("out of ids"));
            }
            (image, id)
        });
        egui::TextureId::User(id)
    }

    /// Removes the image handle and an Egui texture id associated with it.
    pub fn remove_image(&mut self, image: impl Into<AssetId<Image>>) -> Option<egui::TextureId> {
        let image = image.into();
        let id = self.textures.remove(&image);
        log::debug!("Remove image (id: {:?}, handle: {:?})", id, image);
        if let Some((_, id)) = id {
            self.free_list.push(id);
        }
        id.map(|(_, id)| egui::TextureId::User(id))
    }

    /// Returns an associated Egui texture id.
    #[must_use]
    pub fn image_id(&self, image: impl Into<AssetId<Image>>) -> Option<egui::TextureId> {
        let image = image.into();
        self.textures
            .get(&image)
            .map(|&(_, id)| egui::TextureId::User(id))
    }
}

/// A wrapper type for an image handle or an asset id.
#[derive(Clone, Debug)]
pub enum EguiTextureHandle {
    /// Strong handle to an image.
    Strong(Handle<Image>),
    /// Weak handle to an image.
    Weak(AssetId<Image>),
}

impl EguiTextureHandle {
    /// Returns an [`AssetId`] of a wrapped handle.
    pub fn asset_id(&self) -> AssetId<Image> {
        match self {
            EguiTextureHandle::Strong(handle) => handle.id(),
            EguiTextureHandle::Weak(asset_id) => *asset_id,
        }
    }
}

impl From<EguiTextureHandle> for AssetId<Image> {
    fn from(value: EguiTextureHandle) -> Self {
        value.asset_id()
    }
}

/// Stores physical size and scale factor.
#[derive(Component, Debug, Default, Clone, Copy, PartialEq)]
pub struct RenderComputedScaleFactor {
    /// Scale factor.
    pub scale_factor: f32,
}

/// The names of debug_ui nodes.
pub mod node {
    /// The main egui pass.
    pub const EGUI_PASS: &str = "egui_pass";
}

#[derive(SystemSet, Clone, Hash, Debug, Eq, PartialEq)]
/// The plugin startup system sets.
pub enum EguiStartupSet {
    /// Initializes a primary Egui context.
    InitContexts,
}

/// System sets that run during the [`PreUpdate`] schedule.
#[derive(SystemSet, Clone, Hash, Debug, Eq, PartialEq)]
pub enum EguiPreUpdateSet {
    /// Initializes Egui contexts for newly created render targets.
    InitContexts,
    /// Reads Egui inputs and writes them into the [`EguiInput`] resource.
    ProcessInput,
    /// Begins the `egui` pass.
    BeginPass,
}

/// Subsets of the [`EguiPreUpdateSet::ProcessInput`] set.
#[derive(SystemSet, Clone, Hash, Debug, Eq, PartialEq)]
pub enum EguiInputSet {
    /// Reads key modifiers state and pointer positions.
    InitReading,
    /// Processes window mouse button click and touch messages.
    FocusContext,
    /// Processes rest of the messages.
    ReadBevyMessages,
    /// Feeds all the events into [`EguiInput`].
    WriteEguiEvents,
}

/// System sets that run during the [`PostUpdate`] schedule.
#[derive(SystemSet, Clone, Hash, Debug, Eq, PartialEq)]
pub enum EguiPostUpdateSet {
    /// Ends Egui pass.
    EndPass,
    /// Processes Egui output, reads paint jobs for the renderer.
    ProcessOutput,
    /// Post-processing of Egui output.
    PostProcessOutput,
}

impl Plugin for EguiPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<EguiGlobalSettings>();
        app.register_type::<EguiContextSettings>();
        app.init_resource::<EguiGlobalSettings>();
        app.init_resource::<ModifierKeysState>();
        app.init_resource::<EguiWantsInput>();
        app.init_resource::<WindowToEguiContextMap>();
        app.add_message::<EguiInputEvent>();
        app.add_message::<input::EguiFileDragAndDropMessage>();

        app.init_resource::<EguiManagedTextures>();
        app.init_resource::<EguiUserTextures>();
        app.add_plugins(ExtractResourcePlugin::<EguiUserTextures>::default());
        app.add_plugins(ExtractResourcePlugin::<
            render::systems::ExtractedEguiManagedTextures,
        >::default());

        app.configure_sets(
            PreUpdate,
            (
                EguiPreUpdateSet::InitContexts,
                EguiPreUpdateSet::ProcessInput.after(InputSystems),
                EguiPreUpdateSet::BeginPass,
            )
                .chain(),
        );
        app.configure_sets(
            PreUpdate,
            (
                EguiInputSet::InitReading,
                EguiInputSet::FocusContext,
                EguiInputSet::ReadBevyMessages,
                EguiInputSet::WriteEguiEvents,
            )
                .chain(),
        );
        app.configure_sets(
            PostUpdate,
            (
                EguiPostUpdateSet::EndPass,
                EguiPostUpdateSet::ProcessOutput,
                EguiPostUpdateSet::PostProcessOutput,
            )
                .chain(),
        );

        // Startup systems
        app.add_systems(
            PreStartup,
            (
                (setup_primary_egui_context_system, ApplyDeferred)
                    .run_if(|s: Res<EguiGlobalSettings>| s.auto_create_primary_context),
                update_ui_size_and_scale_system,
            )
                .chain()
                .in_set(EguiStartupSet::InitContexts),
        );

        // PreUpdate systems
        app.add_systems(
            PreUpdate,
            (
                setup_primary_egui_context_system
                    .run_if(|s: Res<EguiGlobalSettings>| s.auto_create_primary_context),
                WindowToEguiContextMap::on_egui_context_added_system,
                WindowToEguiContextMap::on_egui_context_removed_system,
                ApplyDeferred,
                update_ui_size_and_scale_system,
            )
                .chain()
                .in_set(EguiPreUpdateSet::InitContexts),
        );
        // NOTE: Replaced bevy_egui's Bevy-message input systems with custom InputEventBuffer reader
        // The old systems expected Bevy's InputPlugin messages (CursorMoved, MouseButtonInput, etc.)
        // We disabled InputPlugin since we own winit, so we read from InputEventBuffer instead
        // But we still need write_egui_input_system to consume EguiInputEvent messages
        app.add_systems(
            PreUpdate,
            (
                input::custom_input_system,
                input::write_egui_input_system,
            )
                .chain()
                .in_set(EguiPreUpdateSet::ProcessInput),
        );
        app.add_systems(
            PreUpdate,
            begin_pass_system.in_set(EguiPreUpdateSet::BeginPass),
        );

        // PostUpdate systems
        app.add_systems(
            PostUpdate,
            (run_egui_context_pass_loop_system, end_pass_system)
                .chain()
                .in_set(EguiPostUpdateSet::EndPass),
        );
        app.add_systems(
            PostUpdate,
            (process_output_system, write_egui_wants_input_system)
                .in_set(EguiPostUpdateSet::ProcessOutput),
        );

        app.add_systems(
            PostUpdate,
            update_egui_textures_system.in_set(EguiPostUpdateSet::PostProcessOutput),
        )
        .add_systems(
            Render,
            render::systems::prepare_egui_transforms_system.in_set(RenderSystems::Prepare),
        )
        .add_systems(
            Render,
            render::systems::queue_bind_groups_system.in_set(RenderSystems::Queue),
        )
        .add_systems(
            Render,
            render::systems::queue_pipelines_system.in_set(RenderSystems::Queue),
        )
        .add_systems(Last, free_egui_textures_system);

        load_internal_asset!(
            app,
            render::EGUI_SHADER_HANDLE,
            "render/egui.wgsl",
            Shader::from_wgsl
        );

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        let egui_graph_2d = render::get_egui_graph(render_app);
        let egui_graph_3d = render::get_egui_graph(render_app);
        let mut graph = render_app
            .world_mut()
            .resource_mut::<bevy::render::render_graph::RenderGraph>();

        if let Some(graph_2d) =
            graph.get_sub_graph_mut(bevy::core_pipeline::core_2d::graph::Core2d)
        {
            graph_2d.add_sub_graph(render::graph::SubGraphEgui, egui_graph_2d);
            graph_2d.add_node(
                render::graph::NodeEgui::EguiPass,
                render::RunEguiSubgraphOnEguiViewNode,
            );
            graph_2d.add_node_edge(
                bevy::core_pipeline::core_2d::graph::Node2d::EndMainPass,
                render::graph::NodeEgui::EguiPass,
            );
            graph_2d.add_node_edge(
                bevy::core_pipeline::core_2d::graph::Node2d::EndMainPassPostProcessing,
                render::graph::NodeEgui::EguiPass,
            );
            graph_2d.add_node_edge(
                render::graph::NodeEgui::EguiPass,
                bevy::core_pipeline::core_2d::graph::Node2d::Upscaling,
            );
        }

        if let Some(graph_3d) =
            graph.get_sub_graph_mut(bevy::core_pipeline::core_3d::graph::Core3d)
        {
            graph_3d.add_sub_graph(render::graph::SubGraphEgui, egui_graph_3d);
            graph_3d.add_node(
                render::graph::NodeEgui::EguiPass,
                render::RunEguiSubgraphOnEguiViewNode,
            );
            graph_3d.add_node_edge(
                bevy::core_pipeline::core_3d::graph::Node3d::EndMainPass,
                render::graph::NodeEgui::EguiPass,
            );
            graph_3d.add_node_edge(
                bevy::core_pipeline::core_3d::graph::Node3d::EndMainPassPostProcessing,
                render::graph::NodeEgui::EguiPass,
            );
            graph_3d.add_node_edge(
                render::graph::NodeEgui::EguiPass,
                bevy::core_pipeline::core_3d::graph::Node3d::Upscaling,
            );
        }
    }

    fn finish(&self, app: &mut App) {
        if let Some(render_app) = app.get_sub_app_mut(RenderApp) {
            render_app
                .insert_resource(render::EguiRenderSettings {
                    bindless_mode_array_size: self.bindless_mode_array_size,
                })
                .init_resource::<render::EguiPipeline>()
                .init_resource::<SpecializedRenderPipelines<render::EguiPipeline>>()
                .init_resource::<render::systems::EguiTransforms>()
                .init_resource::<render::systems::EguiRenderData>()
                .add_systems(
                    ExtractSchedule,
                    render::extract_egui_camera_view_system,
                )
                .add_systems(
                    Render,
                    render::systems::prepare_egui_transforms_system.in_set(RenderSystems::Prepare),
                )
                .add_systems(
                    Render,
                    render::systems::prepare_egui_render_target_data_system
                        .in_set(RenderSystems::Prepare),
                )
                .add_systems(
                    Render,
                    render::systems::queue_bind_groups_system.in_set(RenderSystems::Queue),
                )
                .add_systems(
                    Render,
                    render::systems::queue_pipelines_system.in_set(RenderSystems::Queue),
                );
        }
    }
}

fn input_system_is_enabled(
    test: impl Fn(&EguiInputSystemSettings) -> bool,
) -> impl Fn(Res<EguiGlobalSettings>) -> bool {
    move |settings| test(&settings.input_system_settings)
}

/// Contains textures allocated and painted by Egui.
#[derive(Resource, Deref, DerefMut, Default)]
pub struct EguiManagedTextures(pub HashMap<(Entity, u64), EguiManagedTexture>);

/// Represents a texture allocated and painted by Egui.
pub struct EguiManagedTexture {
    /// Assets store handle.
    pub handle: Handle<Image>,
    /// Stored in full so we can do partial updates.
    pub color_image: egui::ColorImage,
}

/// Adds bevy_egui components to the first camera (primary context).
pub fn setup_primary_egui_context_system(
    mut commands: Commands,
    new_cameras: Query<(Entity, Option<&EguiContext>), Added<bevy::camera::Camera>>,
    mut egui_context_exists: Local<bool>,
) -> Result {
    for (camera_entity, context) in new_cameras {
        log::info!("setup_primary_egui_context_system: processing camera {:?}", camera_entity);

        if context.is_some() || *egui_context_exists {
            log::info!("setup_primary_egui_context_system: skipping camera {:?}, context already exists", camera_entity);
            *egui_context_exists = true;
            return Ok(());
        }

        // Let egui use its default visuals (like official bevy_egui)
        // Do NOT override theme - egui will auto-detect system theme
        let context = EguiContext::default();

        log::info!("Creating a primary Egui context for camera {:?}", camera_entity);
        let mut camera_commands = commands.get_entity(camera_entity)?;
        camera_commands.insert((context, PrimaryEguiContext));
        camera_commands.insert(EguiMultipassSchedule::new(EguiPrimaryContextPass));
        *egui_context_exists = true;
    }

    Ok(())
}

#[derive(QueryData)]
#[query_data(mutable)]
#[allow(missing_docs)]
pub struct UpdateUiSizeAndScaleQuery {
    ctx: &'static mut EguiContext,
    egui_input: &'static mut EguiInput,
    egui_settings: &'static EguiContextSettings,
    camera: &'static bevy::camera::Camera,
}

/// Updates UI screen_rect and pixels_per_point.
pub fn update_ui_size_and_scale_system(mut contexts: Query<UpdateUiSizeAndScaleQuery>) {
    for mut context in contexts.iter_mut() {
        let Some((scale_factor, viewport_rect)) = context
            .camera
            .target_scaling_factor()
            .map(|scale_factor| scale_factor * context.egui_settings.scale_factor)
            .zip(context.camera.physical_viewport_rect())
        else {
            continue;
        };

        let viewport_rect = egui::Rect {
            min: helpers::vec2_into_egui_pos2(viewport_rect.min.as_vec2() / scale_factor),
            max: helpers::vec2_into_egui_pos2(viewport_rect.max.as_vec2() / scale_factor),
        };
        if viewport_rect.width() < 1.0 || viewport_rect.height() < 1.0 {
            continue;
        }

        // DIAGNOSTIC: Check screen_rect being set
        log::warn!(
            "Setting egui screen_rect: {:?}, scale_factor: {}, physical_viewport: {:?}",
            viewport_rect,
            scale_factor,
            context.camera.physical_viewport_rect()
        );

        context.egui_input.screen_rect = Some(viewport_rect);
        context.ctx.get_mut().set_pixels_per_point(scale_factor);
    }
}

/// Marks a pass start for Egui.
pub fn begin_pass_system(
    mut contexts: Query<
        (&mut EguiContext, &EguiContextSettings, &mut EguiInput),
        Without<EguiMultipassSchedule>,
    >,
) {
    let count = contexts.iter().count();
    if count > 0 {
        log::info!("begin_pass_system: processing {} contexts", count);
    }
    for (mut ctx, egui_settings, mut egui_input) in contexts.iter_mut() {
        if !egui_settings.run_manually {
            ctx.get_mut().begin_pass(egui_input.take());
        }
    }
}

/// Marks a pass end for Egui.
pub fn end_pass_system(
    mut contexts: Query<
        (&mut EguiContext, &EguiContextSettings, &mut EguiFullOutput),
        Without<EguiMultipassSchedule>,
    >,
) {
    let count = contexts.iter().count();
    if count > 0 {
        log::info!("end_pass_system: processing {} contexts", count);
    }
    for (mut ctx, egui_settings, mut full_output) in contexts.iter_mut() {
        if !egui_settings.run_manually {
            **full_output = Some(ctx.get_mut().end_pass());
            log::info!("end_pass_system: generated full_output");
        }
    }
}

#[derive(QueryData)]
#[query_data(mutable)]
#[allow(missing_docs)]
pub struct MultiPassEguiQuery {
    entity: Entity,
    context: &'static mut EguiContext,
    input: &'static mut EguiInput,
    output: &'static mut EguiFullOutput,
    multipass_schedule: &'static EguiMultipassSchedule,
    settings: &'static EguiContextSettings,
}

/// Runs Egui contexts with the [`EguiMultipassSchedule`] component.
pub fn run_egui_context_pass_loop_system(world: &mut World) {
    let mut contexts_query = world.query::<MultiPassEguiQuery>();
    let mut used_schedules = HashSet::<InternedScheduleLabel>::default();

    let mut multipass_contexts: Vec<_> = contexts_query
        .iter_mut(world)
        .filter_map(|mut egui_context| {
            if egui_context.settings.run_manually {
                return None;
            }

            Some((
                egui_context.entity,
                egui_context.context.get_mut().clone(),
                egui_context.input.take(),
                egui_context.multipass_schedule.clone(),
            ))
        })
        .collect();

    if !multipass_contexts.is_empty() {
        log::info!("run_egui_context_pass_loop_system: processing {} contexts", multipass_contexts.len());
    }

    for (entity, ctx, input, EguiMultipassSchedule(multipass_schedule)) in &mut multipass_contexts {
        if !used_schedules.insert(*multipass_schedule) {
            panic!(
                "Each Egui context running in the multi-pass mode must have a unique schedule (attempted to reuse schedule {multipass_schedule:?})"
            );
        }

        // DIAGNOSTIC: Check input being passed to run()
        let raw_input = input.take();
        log::warn!(
            "Calling ctx.run() with screen_rect: {:?}",
            raw_input.screen_rect
        );

        let output = ctx.run(raw_input, |_| {
            let _ = world.try_run_schedule(*multipass_schedule);
        });

        // DIAGNOSTIC: Check fonts after run()
        ctx.fonts(|fonts| {
            let num_families = fonts.families().len();
            log::warn!("After run(), context has {} font families", num_families);
        });

        log::info!("run_egui_context_pass_loop_system: generated output for entity {:?}", entity);

        **contexts_query
            .get_mut(world, *entity)
            .expect("previously queried context")
            .output = Some(output);
    }

    // Run the primary schedule if it hasn't been run yet
    if world
        .query_filtered::<Entity, (With<EguiContext>, With<PrimaryEguiContext>)>()
        .iter(world)
        .next()
        .is_none()
    {
        return;
    }
    if !used_schedules.contains(&ScheduleLabel::intern(&EguiPrimaryContextPass)) {
        let _ = world.try_run_schedule(EguiPrimaryContextPass);
    }
}

/// Updates textures painted by Egui.
pub fn update_egui_textures_system(
    mut egui_render_output: Query<(Entity, &EguiRenderOutput)>,
    mut egui_managed_textures: ResMut<EguiManagedTextures>,
    mut image_assets: ResMut<Assets<Image>>,
) {
    use bevy::image::TextureAccessError;

    for (entity, egui_render_output) in egui_render_output.iter_mut() {
        if !egui_render_output.textures_delta.set.is_empty() {
            log::info!("update_egui_textures_system: {} texture updates", egui_render_output.textures_delta.set.len());
        }
        for (texture_id, image_delta) in &egui_render_output.textures_delta.set {
            let color_image = render::as_color_image(&image_delta.image);

            let texture_id = match texture_id {
                egui::TextureId::Managed(texture_id) => *texture_id,
                egui::TextureId::User(_) => continue,
            };

            let sampler = ImageSampler::Descriptor(render::texture_options_as_sampler_descriptor(
                &image_delta.options,
            ));
            if let Some(pos) = image_delta.pos {
                // Partial update
                if let Some(managed_texture) = egui_managed_textures.get_mut(&(entity, texture_id))
                    && let Some(image) = image_assets.get_mut(managed_texture.handle.id())
                {
                    if update_image_rect(image, pos, &color_image).is_err() {
                        log::error!(
                            "Failed to write into texture (id: {:?}) for partial update",
                            texture_id
                        );
                    }
                } else {
                    log::warn!("Partial update of a missing texture (id: {:?})", texture_id);
                }
            } else {
                // Full update
                let image = render::color_image_as_bevy_image(&color_image, sampler);
                let handle = image_assets.add(image);
                log::info!("update_egui_textures_system: created texture {:?} ({}x{})",
                    texture_id, color_image.width(), color_image.height());
                egui_managed_textures.insert(
                    (entity, texture_id),
                    EguiManagedTexture {
                        handle,
                        color_image,
                    },
                );
            }
        }
    }

    fn update_image_rect(
        dest: &mut Image,
        [x, y]: [usize; 2],
        src: &egui::ColorImage,
    ) -> Result<(), TextureAccessError> {
        for sy in 0..src.height() {
            for sx in 0..src.width() {
                let px = src[(sx, sy)];

                dest.set_color_at(
                    (x + sx) as u32,
                    (y + sy) as u32,
                    bevy::color::Color::srgba_u8(px.r(), px.g(), px.b(), px.a()),
                )?;
            }
        }

        Ok(())
    }
}

/// Frees Egui-managed textures and user textures.
pub fn free_egui_textures_system(
    mut egui_user_textures: ResMut<EguiUserTextures>,
    egui_render_output: Query<(Entity, &EguiRenderOutput)>,
    mut egui_managed_textures: ResMut<EguiManagedTextures>,
    mut image_assets: ResMut<Assets<Image>>,
    mut image_event_reader: MessageReader<AssetEvent<Image>>,
) {
    for (entity, egui_render_output) in egui_render_output.iter() {
        for &texture_id in &egui_render_output.textures_delta.free {
            if let egui::TextureId::Managed(texture_id) = texture_id {
                let managed_texture = egui_managed_textures.remove(&(entity, texture_id));
                if let Some(managed_texture) = managed_texture {
                    image_assets.remove(&managed_texture.handle);
                }
            }
        }
    }

    for message in image_event_reader.read() {
        if let AssetEvent::Removed { id } = message {
            egui_user_textures.remove_image(EguiTextureHandle::Weak(*id));
        }
    }
}
