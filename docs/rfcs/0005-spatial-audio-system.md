# RFC 0005: Spatial Audio System

**Status:** Draft
**Authors:** Sienna
**Created:** 2025-12-14

## Abstract

This RFC proposes vendoring Firewheel (audio graph engine) and Steam Audio (spatial simulation) into Marathon's engine layer, providing a bus-based mixing architecture with developer tooling for professional-grade audio mixing. The system handles spatial 3D audio, real-time environmental simulation, and provides egui-based debugging tools for mix engineering.

## Motivation

The current engine lacks audio infrastructure entirely. Aspen's design requirements specify professional-grade spatial audio with "hyper-dense" soundscapes, which requires more than bevy_audio's basic playback capabilities.

### Current Options and Trade-offs

**bevy_audio** — Bevy's built-in audio system provides basic playback but no spatial simulation, no mixing infrastructure, and no developer tooling. Insufficient for professional audio work.

**Middleware (FMOD/Wwise)** — Industry-standard solutions with excellent tooling, but introduce external editors, licensing complexity, poor Rust integration, and proprietary formats. The friction of context-switching between middleware and game engine slows iteration.

**bevy_seedling + bevy_steam_audio** — Rust-native solutions that integrate Firewheel (lock-free audio graph) and Steam Audio (binaural spatial simulation). These are the right foundation, but as external dependencies they lag Bevy version updates and don't integrate with Marathon's conventions.

### Solution: Vendor and Integrate

Vendor Firewheel and Steam Audio as first-party Marathon code. This provides:

1. **Version control** — Update Bevy on our schedule, not waiting for upstream
2. **Simplification** — Strip unused features (LUFS analyzer nodes, input stream handling, complex pool abstractions)
3. **Integration** — Use Marathon's logging, diagnostics, and patterns throughout
4. **Tooling** — Build egui mixer panels and spatial visualization that understand Marathon's conventions

The maintenance burden (porting upstream improvements manually) is accepted in exchange for control and simplicity.

### Requirements

From Aspen's audio direction:

1. **3D spatial audio** — Sounds positioned in world space, processed through HRTF binaural simulation
2. **Hyper-dense soundscapes** — Hundreds of potential sources with intelligent prioritization and culling
3. **Professional mixing** — Bus-based architecture with EQ, metering, and preset management
4. **Real-time simulation** — Steam Audio for distance attenuation, occlusion, and (future) reverb

From Marathon's engine requirements:

5. **First-party code** — Vendored into Marathon, not external dependencies
6. **Developer tooling** — egui mixer panel and spatial debug visualization
7. **Performance** — Handle 64+ simultaneous voices at <2ms audio thread latency

## Architecture

The audio system forms a pipeline from game world to speaker output:

```
[Game Layer: AudioSource + Transform components]
           ↓
[Bevy Integration: Position sync, asset loading, component lifecycle]
           ↓
[Bus Mixer: SFX, Ambient, Music, UI, Voice → Master]
           ↓
[Firewheel: Lock-free audio graph, real-time thread]
           ↓
[Steam Audio: HRTF binaural, distance attenuation, occlusion]
           ↓
[cpal: Audio I/O] → Speakers/Headphones
```

**Game Layer** — Games spawn entities with `AudioSource` and `Transform` components. The engine handles everything else.

**Bevy Integration** — Synchronizes ECS state with the audio graph. When transforms change, update Steam Audio parameters. When entities spawn, create corresponding Firewheel nodes.

**Bus Mixer** — Categorical organization (SFX, Ambient, Music, UI, Voice). Each bus has gain, EQ, sends. Master bus applies limiting and metering.

**Firewheel** — Lock-free audio graph engine running on a dedicated real-time thread. Processes ~512-sample buffers at 48kHz (~10ms per buffer).

**Steam Audio** — Spatial simulation providing HRTF convolution, distance attenuation, air absorption, and occlusion detection.

## Audio Source Lifecycle

When a game spawns an entity with `AudioSource` and `Transform`:

1. **Detection** — `Added<AudioSource>` query in Bevy system detects new entity
2. **Node Creation** — Create Firewheel nodes: sampler → Steam Audio processor → gain → bus input
3. **Graph Connection** — Connect nodes in the audio graph (lock-free operation)
4. **Parameter Sync** — Every frame, `Changed<Transform>` updates Steam Audio position/direction atomics
5. **Audio Thread Processing** — Real-time thread reads atomics, processes buffers, outputs spatialized audio
6. **Despawn** — `RemovedComponents<AudioSource>` disconnects and removes nodes

The critical design is **two-thread architecture with lock-free communication**:

- **Game thread (60Hz)** — Updates parameters via atomics, never blocks audio
- **Audio thread (100Hz)** — Reads atomics, processes buffers, never blocks on game state

The audio thread always has valid parameters (possibly slightly stale). This prevents glitches from game thread hitches.

## Spatial Audio Processing

Steam Audio transforms mono sources into binaural stereo through four stages:

**1. Distance Attenuation** — Volume falloff based on distance. Curve is configurable (linear, logarithmic, custom) between min distance (full volume) and max distance (silent).

**2. Air Absorption** — Frequency-dependent filtering. High frequencies attenuate faster over distance. Modeled with 3-band filter (low/mid/high). This is why distant thunder rumbles but nearby thunder cracks.

**3. Occlusion** — Ray-casting determines if geometry blocks the direct path. Occlusion factor (0.0 = fully blocked, 1.0 = clear) applies low-pass filtering to simulate transmission through materials.

**4. HRTF Convolution** — Convolves the mono source with Head-Related Transfer Functions for the source's direction. Creates stereo output that the brain interprets as coming from that direction in space. Uses Steam Audio's default HRTF measured from dummy head, with support for custom HRTFs via SOFA files.

The result: a mono sound positioned in world space becomes stereo that sounds like it's coming from that position when heard through headphones.

## Bus Mixer

Instead of mixing hundreds of sources individually, we mix categorical buses:

- **SFX Bus** — Footsteps, hammer hits, impacts
- **Ambient Bus** — Wind, birds, streams, background loops
- **Music Bus** — Background music stems
- **UI Bus** — Button clicks, menu sounds
- **Voice Bus** — Dialogue, narration (future)

Each bus provides:

- **3-band EQ** — Low shelf, mid bell, high shelf for tonal shaping
- **Send levels** — Route to effect buses (reverb, delay)
- **Fader** — Gain control, -∞ to +12dB
- **Solo/Mute** — Isolation for debugging
- **Metering** — Real-time peak and RMS

**Master Bus** adds:

- **Limiter** — Peak limiting at -0.3dB ceiling (prevents clipping)
- **LUFS Meter** — Integrated loudness (target: -14 LUFS for streaming platforms)

Sound engineers think in categories ("SFX is too loud") rather than individual sources. This architecture matches that mental model.

## Source Prioritization

With potentially 200+ sources in a dense scene, we can only render ~64 voices before mix becomes muddy and CPU maxes out. Priority system determines which sources actually play:

**Culling pipeline:**

1. **Distance cull** — Sources beyond max distance are culled immediately
2. **Amplitude cull** — Sources below audibility threshold are culled
3. **Priority scoring** — Score remaining sources based on:
   - Bus type (Voice > UI > SFX > Music > Ambient)
   - Distance (closer = higher priority)
   - Amplitude (louder = higher priority)
   - Recency (recently started sounds get boost to preserve transients)
4. **Sort by priority** — Highest to lowest
5. **Take top N** — Render top 64, cull the rest

Result: In crowded scenes, important sounds stay clear. Ambient background gracefully fades when foreground action demands attention.

## Soundscape Zones

For hyper-dense ambient audio, individual point sources are insufficient. Soundscape zones define regions that activate layered sounds when the listener enters:

**SoundscapeZone component:**

- **Shape** — Sphere, box, or cylinder
- **Layers** — Multiple sound sources (e.g., wind rustling leaves, distant birds, occasional creaks)
- **Fade distance** — Crossfade range when entering/exiting (prevents popping)
- **Priority** — When zones overlap, higher priority wins

**Layer types:**

- **Spatial** — Positioned in world, processed through Steam Audio
- **Non-spatial** — Plays directly to bus without spatialization (ambient beds)
- **Randomized** — Plays occasionally with random timing (birds, creaks)

The system automatically activates/deactivates zones based on listener position and crossfades between overlapping zones.

## Developer Tooling

### Spatial Debug Visualization

Egui-based debug mode renders active audio sources as translucent spheres in world space. Uses Bevy gizmos for simplicity (not volumetric shaders).

Each visualization shows:

- **Position** — Sphere centered on source's world position
- **Falloff range** — Two concentric spheres (min distance / max distance)
- **Amplitude** — Brightness pulses with current amplitude
- **Bus category** — Color-coded by bus (SFX = blue, Ambient = green, Music = purple, UI = yellow, Voice = orange)
- **Occlusion** — Ray from source to listener (green = clear, red = occluded)

**Selection workflow:**

1. Click on sphere gizmo
2. Inspector panel shows: amplitude (animated bar), occlusion %, distance, bus, spatial config
3. Action buttons: "Solo This Bus", "Open in Mixer", "Play Solo"

**Pause and inspect:**

1. Pause game
2. Visualizations freeze in place
3. Click sources to inspect state
4. Adjust mixer
5. Resume to hear changes

The tight loop (hear problem → see problem → fix problem) is what separates professional tooling from guesswork.

### Mixer Panel

Egui panel resembling a hardware mixing console. Each bus gets a channel strip:

```
┌────────┐
│  SFX   │
├────────┤
│ EQ     │ — 3-band controls (collapsed by default)
│ Sends  │ — Reverb/delay levels
│ [S][M] │ — Solo/mute buttons
│ ▮▮▮▯▯▯ │ — Peak meter
│ ┃████┃ │ — Fader
│ -2.0dB │ — Gain readout
└────────┘
```

**Master section:**

```
┌──────────┐
│  MASTER  │
├──────────┤
│ Limiter  │
│ [ON] -0.3│
├──────────┤
│ ▮▮▮▮▮▯▯▯ │ — Stereo meter
│ ┃█████┃  │ — Fader
│  0.0dB   │
├──────────┤
│LUFS: -14 │ — Integrated loudness
└──────────┘
```

**Preset system:**

- Save complete mixer state (all fader positions, EQ settings, sends)
- Serialize to JSON (version-controllable alongside assets)
- A/B toggle for comparison

## Data Structures

### Core Components

```rust
/// Positioned audio source in world space
#[derive(Component)]
pub struct AudioSource {
    pub sample: Handle<AudioSample>,
    pub bus: AudioBus,
    pub gain: f32,
    pub looping: bool,
    pub spatial: SpatialConfig,
}

#[derive(Clone)]
pub struct SpatialConfig {
    pub min_distance: f32,
    pub max_distance: f32,
    pub falloff: FalloffCurve,
    pub occlusion_enabled: bool,
}

#[derive(Component)]
pub struct AudioListener;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AudioBus {
    #[default]
    Sfx,
    Ambient,
    Music,
    Ui,
    Voice,
}
```

### Soundscape Zones

```rust
#[derive(Component)]
pub struct SoundscapeZone {
    pub layers: Vec<SoundscapeLayer>,
    pub shape: ZoneShape,
    pub fade_distance: f32,
}

pub struct SoundscapeLayer {
    pub sample: Handle<AudioSample>,
    pub gain: f32,
    pub spatial: bool,
}

pub enum ZoneShape {
    Sphere { radius: f32 },
    Box { half_extents: Vec3 },
    Cylinder { radius: f32, height: f32 },
}
```

### Mixer State

```rust
#[derive(Resource)]
pub struct MixerState {
    pub buses: HashMap<AudioBus, BusState>,
    pub effects: Vec<EffectBusState>,
    pub master: MasterState,
}

pub struct BusState {
    pub gain_db: f32,
    pub muted: bool,
    pub soloed: bool,
    pub eq: BusEq,
    pub sends: Vec<SendLevel>,

    // Read-only, updated from audio thread via atomics
    pub peak_l: f32,
    pub peak_r: f32,
    pub rms_l: f32,
    pub rms_r: f32,
}

pub struct MasterState {
    pub gain_db: f32,
    pub limiter_enabled: bool,
    pub limiter_ceiling_db: f32,
    pub peak_l: f32,
    pub peak_r: f32,
    pub lufs_integrated: f32,
}
```

### Debug Visualization

```rust
#[derive(Resource, Default)]
pub struct AudioDebugState {
    pub listener: Vec3,
    pub sources: Vec<AudioSourceDebug>,
}

pub struct AudioSourceDebug {
    pub entity: Entity,
    pub position: Vec3,
    pub bus: AudioBus,
    pub amplitude: f32,
    pub min_distance: f32,
    pub max_distance: f32,
    pub occlusion: f32,
}

#[derive(Resource)]
pub struct AudioDebugSettings {
    pub enabled: bool,
    pub show_falloff_ranges: bool,
    pub show_occlusion_rays: bool,
    pub min_amplitude_threshold: f32,
    pub bus_colors: HashMap<AudioBus, Color>,
}
```

## Implementation Phases

### Phase 1: Vendor Firewheel

- Fork Firewheel into `crates/libmarathon/src/audio/firewheel/`
- Strip unused features (LUFS nodes, input streams, complex pooling)
- Adapt to Marathon's logging patterns
- Verify basic playback works

**Critical files:**
- NEW: `crates/libmarathon/src/audio/firewheel/` (vendored)
- NEW: `crates/libmarathon/src/audio/mod.rs` (module root)

### Phase 2: Vendor Steam Audio

- Vendor Steam Audio Rust bindings (audionimbus) into `crates/libmarathon/src/audio/steam_audio/`
- Create Firewheel node wrapper for Steam Audio processing
- Implement HRTF initialization (default dummy head HRTF)
- Test binaural output with positioned source

**Critical files:**
- NEW: `crates/libmarathon/src/audio/steam_audio/` (vendored bindings)
- NEW: `crates/libmarathon/src/audio/steam_audio_node.rs` (Firewheel integration)

### Phase 3: Bevy Integration

- Create `AudioSource`, `AudioListener` components
- Implement position sync system (Transform → Steam Audio atomics)
- Implement component lifecycle (Added/Removed → node creation/cleanup)
- Asset loading (decode audio files into Firewheel samples)

**Critical files:**
- NEW: `crates/libmarathon/src/audio/components.rs`
- NEW: `crates/libmarathon/src/audio/systems.rs`
- NEW: `crates/libmarathon/src/audio/assets.rs`

### Phase 4: Bus Mixer

- Create `MixerState` resource with bus hierarchy
- Implement bus Firewheel nodes with gain/EQ/sends
- Connect all sources to appropriate buses
- Add master bus with limiting

**Critical files:**
- NEW: `crates/libmarathon/src/audio/mixer.rs`
- NEW: `crates/libmarathon/src/audio/buses.rs`

### Phase 5: Prioritization and Culling

- Implement priority scoring system
- Add distance and amplitude culling
- Enforce voice limit (64 simultaneous)
- Test with 200+ sources

**Critical files:**
- NEW: `crates/libmarathon/src/audio/prioritization.rs`
- MODIFY: `crates/libmarathon/src/audio/systems.rs`

### Phase 6: Debug Visualization

- Implement gizmo rendering for active sources
- Add selection raycasting
- Create inspector panel (egui)
- Add amplitude animation to visualizations

**Critical files:**
- NEW: `crates/libmarathon/src/audio/debug.rs`
- NEW: `crates/libmarathon/src/audio/debug_ui.rs`

### Phase 7: Mixer Panel

- Implement egui mixer panel with channel strips
- Add EQ controls (collapsed by default)
- Add solo/mute buttons
- Implement metering (peak/RMS from audio thread)
- Add preset save/load

**Critical files:**
- NEW: `crates/libmarathon/src/audio/mixer_ui.rs`
- NEW: `crates/libmarathon/src/audio/presets.rs`

### Phase 8: Soundscape Zones

- Implement `SoundscapeZone` component
- Add zone activation system (listener position → zone enable/disable)
- Implement crossfading between zones
- Add randomized layer playback

**Critical files:**
- NEW: `crates/libmarathon/src/audio/soundscape.rs`

## Design Decisions

### Why Vendor Instead of Depend?

**Decision:** Copy bevy_seedling and bevy_steam_audio source into Marathon, adapt to Marathon's patterns.

**Rationale:**

External dependencies create version lag (we wait for Bevy compatibility updates), include unused features (bloat), and don't match Marathon's conventions (logging, diagnostics). Vendoring provides control (update Bevy on our schedule), simplicity (strip unused code), and integration (use Marathon's patterns).

**Trade-off:** Maintenance burden (manually port upstream improvements) in exchange for control and integration.

### Why Real-Time Simulation (Not Baked)?

**Decision:** Use Steam Audio's real-time simulation initially. Add baking as optimization later if needed.

**Rationale:**

Baked audio pre-computes reverb at probe points, saving CPU but requiring build steps and consuming memory (~hundreds of MB for dense grids). Baked data doesn't handle dynamic geometry.

Aspen has construction and terrain modification — players actively change the world. Baked data would need frequent rebuilding. Real-time handles this naturally and allows immediate iteration.

**Risk:** CPU cost. Steam Audio's real-time reverb is expensive.

**Mitigation:** If we hit performance limits, add baked reverb for static geometry while keeping real-time for dynamic elements.

**Trade-off:** Higher CPU usage in exchange for dynamic world support and faster iteration.

### Why Bus-Based Mixing?

**Decision:** Expose mixing controls at bus level (5 buses), not individual sources (200+ sources).

**Rationale:**

Sound engineers think categorically ("ambient is too loud") not granularly ("this specific wind loop at position X is too loud"). With 200+ sources, individual control is cognitively impossible.

Industry tools (FMOD, Wwise, Pro Tools) all use bus/group architecture. This matches existing mental models.

Individual source control is still available through inspector panel for debugging.

**Trade-off:** Less granular control in exchange for cognitive manageability.

### Why Gizmos (Not Volumetric Shaders)?

**Decision:** Use Bevy gizmos for debug visualization initially. Consider custom shaders later if needed.

**Rationale:**

True volumetric hazes (ray-marched spheres) would look better but are expensive and complex. Gizmos are simple, performant, already available in Bevy.

Visualization is for development tooling, not player-facing graphics. Functional clarity > visual polish.

**Trade-off:** Less visual polish in exchange for implementation simplicity.

## Open Questions

### HRTF Personalization

Steam Audio supports custom HRTFs from SOFA files. Personalized HRTFs (measured from individual's ears) provide dramatically better spatial accuracy than generic HRTFs.

Should we expose this as accessibility option? Some players may have hearing differences that make generic HRTFs ineffective.

**Challenge:** SOFA files must be measured or generated. Services exist (HRTF from ear photos) but quality varies.

### Head Tracking on iPad

AirPods Pro provide head orientation via Core Motion. Combined with spatial audio, head tracking anchors sounds in world space as you turn your head, improving externalization.

Should we integrate head tracking? Aspen uses fixed camera in city builder — listener position is camera, not player's head. Does head tracking make sense? Would it be disorienting if sounds move when you physically turn your head while visual camera stays fixed?

**Possible approaches:**
- Disable entirely
- Optional for users who want it
- Hybrid with reduced sensitivity

### Reverb Strategy

Options:
- **Real-time ray-traced** — Handles dynamic geometry, CPU-expensive
- **Baked probe-based** — Cheap at runtime, requires build step, doesn't handle dynamics
- **Parametric (FDN)** — Middle ground, cheaper than convolution, less accurate

For Aspen's construction-heavy gameplay, which is right?

**Possible hybrid:** Baked for large static geometry (terrain), real-time for buildings (player constructs), parametric for fallback.

### Network Sync for Audio

In parallel multiplayer, should audio events sync between peers?

- **If yes:** Need to network events, handle latency, avoid doubling when both hear local and remote
- **If no:** Each player hears only own actions, may feel disconnected

**Possible middle ground:** Sync "world state" sounds (ambient zones, persistent audio) but not transients (footsteps, UI).

### Music System

This RFC focuses on spatial SFX/ambient. Music requires:

- Stem-based playback (mix layers independently)
- Adaptive mixing (respond to game state)
- Crossfading between tracks
- Beat-synced events
- Horizontal re-sequencing

Should we include basic music playback here (non-spatial, single track, simple crossfade) and defer adaptive system, or wait and design music holistically?

## Success Metrics

- **Spatial accuracy:** Blindfolded playtesters point toward sources within 15° error. Front/back confusion <10%.
- **Mix quality:** Professional mix rated "better" by >80% of A/B test participants.
- **Tooling effectiveness:** Sound engineer identifies and fixes deliberate mix problem within 5 minutes using debug visualization.
- **Performance:** 64 simultaneous voices with Steam Audio HRTF at <2ms audio thread time on M1 iPad. No audible glitches.

## Testing Strategy

### Unit Tests

- `test_audio_graph_lifecycle()` — Verify node creation/cleanup on entity spawn/despawn
- `test_bus_routing()` — Verify sources route to correct buses
- `test_prioritization()` — Verify priority scoring and culling
- `test_spatial_config()` — Verify Steam Audio parameter sync from Transform

### Integration Tests

- `test_dense_soundscape()` — Spawn 200 sources, verify 64 voice limit enforced
- `test_zone_activation()` — Listener enters/exits zone, verify layers activate/deactivate
- `test_mixer_preset()` — Save/load preset, verify state restoration
- `test_occlusion()` — Place geometry between source and listener, verify occlusion applied

### Performance Tests

- 64 active voices: <2ms audio thread latency
- 200 total sources: Priority culling to 64 in <1ms
- Rejoin bandwidth (not applicable to audio)

## References

- [Firewheel](https://github.com/BillyDM/firewheel) — Lock-free audio graph engine
- [bevy_seedling](https://github.com/CorvusPrudens/bevy_seedling) — Reference for Firewheel integration patterns
- [audionimbus](https://github.com/MaxenceMaire/audionimbus) — Rust bindings for Steam Audio
- [Steam Audio Documentation](https://valvesoftware.github.io/steam-audio/) — Spatial simulation reference
- Aspen Style Guide, Section 6: Audio Direction
