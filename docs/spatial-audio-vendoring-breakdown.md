# Spatial Audio System - Task Breakdown

**Epic:** Spatial Audio System (#4)
**Overall Size:** XXL+ (91+ points across 8 phases)
**Priority:** P1 (High - core immersion feature)

This document breaks down the 8 phases into specific, sized tasks for prioritization and scheduling.

**Note:** We are re-implementing bevy_seedling and bevy_steam_audio, not forking them. We depend on the underlying libraries (Firewheel and Steam Audio) as external crates, but write our own integration code that follows Marathon's patterns and doesn't lag behind Bevy version updates.

---

## Phase 1: Implement Firewheel Integration

**Phase Goal:** Re-implement bevy_seedling's Firewheel integration for Marathon
**Phase Size:** 14 points
**Dependencies:** None (can start immediately)
**Risk:** Medium (lock-free audio graph integration is complex)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 1.1 | Add Firewheel dependency and create audio module structure | S | 2 | Add crate dependency, set up module hierarchy | P1 |
| 1.2 | Implement audio graph initialization and lifecycle | M | 4 | Create graph, manage real-time thread, handle shutdown | P1 |
| 1.3 | Create sample playback nodes and basic routing | M | 4 | Sampler nodes, gain nodes, basic graph connections | P1 |
| 1.4 | Implement cpal audio output integration | S | 2 | Connect Firewheel graph to system audio output | P1 |
| 1.5 | Verify basic playback works (smoke test) | S | 2 | Test on macOS and iOS, verify no glitches | P1 |

**Phase 1 Total:** 14 points

### Lean Analysis
- **Eliminate Waste:** Can we use bevy_seedling directly? NO - lags Bevy updates, doesn't match Marathon patterns
- **Amplify Learning:** What will we learn? How to integrate lock-free audio graphs with ECS
- **Deliver Fast:** Can we implement incrementally? YES - basic playback first, then add features
- **Build Quality In:** Risk of audio glitches? YES - comprehensive playback testing critical

### Phase 1 Recommendations
1. **Do 1.1-1.4 sequentially** - each builds on previous
2. **Do 1.5 thoroughly** - verify no dropouts, glitches, or latency issues
3. **Reference bevy_seedling** - use it as reference implementation, but write our own code

---

## Phase 2: Implement Steam Audio Integration

**Phase Goal:** Re-implement bevy_steam_audio's Steam Audio integration for Marathon
**Phase Size:** 18 points
**Dependencies:** Phase 1 complete
**Risk:** High (C++ bindings, HRTF complexity)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 2.1 | Add steam-audio dependency (audionimbus bindings) | S | 2 | Add crate dependency, verify C++ library linking | P1 |
| 2.2 | Create Firewheel processor node for Steam Audio | L | 8 | Bridge between Firewheel and Steam Audio APIs, handle FFI safely | P1 |
| 2.3 | Implement HRTF initialization with default dataset | M | 4 | Load MIT KEMAR HRTF, verify initialization | P1 |
| 2.4 | Implement distance attenuation and air absorption | S | 2 | Basic spatial processing before HRTF | P1 |
| 2.5 | Test binaural output with positioned source | S | 2 | Create test scene, verify left/right panning and elevation | P1 |

**Phase 2 Total:** 18 points

### Lean Analysis
- **Eliminate Waste:** Can we use simpler panning? NO - HRTF is core to immersion
- **Amplify Learning:** Should we prototype Steam Audio separately? YES - task 2.5 is critical learning
- **Decide Late:** Can we defer HRTF? NO - it's foundational to spatial audio
- **Optimize Whole:** Does this improve both iOS and macOS? YES - cross-platform from start

### Critical Path
```
2.1 (add dependency)
  ↓
2.2 (Firewheel processor node) → 2.3 (HRTF init)
  ↓
2.4 (distance/air absorption)
  ↓
2.5 (binaural test)
```

### Phase 2 Recommendations
1. **Start with 2.1-2.3 sequentially** - Steam Audio setup is delicate
2. **Test heavily at 2.5** - spatial accuracy is mission-critical
3. **Reference bevy_steam_audio** - use as reference for Steam Audio API usage

---

## Phase 3: Bevy Integration

**Phase Goal:** Connect ECS components to audio graph
**Phase Size:** 20 points
**Dependencies:** Phase 2 complete
**Risk:** Medium (lock-free sync between game thread and audio thread)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 3.1 | Create `AudioSource` and `AudioListener` components | S | 2 | Define component API, derive traits | P1 |
| 3.2 | Implement position sync system (Transform → atomics) | L | 8 | Core sync logic, must be lock-free and glitch-free | P1 |
| 3.3 | Implement component lifecycle (Added/Removed) | M | 4 | Handle entity spawn/despawn, cleanup nodes | P1 |
| 3.4 | Create audio asset loading system | M | 4 | Decode audio files, integrate with Bevy assets | P1 |
| 3.5 | Test with moving sources and listener | S | 2 | Verify Doppler-free position updates | P1 |

**Phase 3 Total:** 20 points

### Lean Analysis
- **Eliminate Waste:** This IS the integration work - no waste
- **Amplify Learning:** Will this reveal audio thread issues? YES - explicit test for it (3.5)
- **Build Quality In:** Test concurrency early? YES - that's the whole phase
- **Deliver Fast:** Can we ship without asset loading (3.4)? NO - need real audio files

### Phase 3 Recommendations
1. **Do 3.1 first** - API design gates everything else
2. **3.2 is the critical path** - most complex, needs careful review
3. **Do 3.5 extensively** - test on real hardware, listen for glitches

---

## Phase 4: Bus Mixer

**Phase Goal:** Implement categorical bus-based mixing
**Phase Size:** 14 points
**Dependencies:** Phase 3 complete
**Risk:** Low (straightforward audio routing)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 4.1 | Create `MixerState` resource with bus hierarchy | S | 2 | Define SFX/Ambient/Music/UI/Voice buses | P1 |
| 4.2 | Implement bus Firewheel nodes (gain, EQ, sends) | L | 8 | Multiple node types, routing complexity | P1 |
| 4.3 | Connect all sources to appropriate buses | S | 2 | Route AudioSource components by bus type | P2 |
| 4.4 | Add master bus with limiting | S | 2 | Prevent clipping, add safety limiter | P1 |
| 4.5 | Test bus gain changes propagate correctly | XS | 1 | Verify mixer controls work | P2 |

**Phase 4 Total:** 15 points

### Lean Analysis
- **Eliminate Waste:** Do we need 5 buses initially? YES - categorical thinking is core
- **Amplify Learning:** Can we defer EQ? NO - it's essential for professional mixing
- **Build Quality In:** Is limiting necessary? YES - prevents painful clipping accidents
- **Optimize Whole:** Does bus structure match sound design needs? YES - aligns with RFC requirements

### Phase 4 Recommendations
1. **4.1 and 4.2 together** - design and implementation are coupled
2. **4.4 is critical** - limiter saves ears during development
3. **Fast phase:** Mostly plumbing once Firewheel is solid

---

## Phase 5: Prioritization and Culling

**Phase Goal:** Handle 200+ sources by prioritizing top 64
**Phase Size:** 12 points
**Dependencies:** Phase 4 complete
**Risk:** Medium (performance-critical code path)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 5.1 | Implement priority scoring system | M | 4 | Distance, amplitude, bus type, recency factors | P1 |
| 5.2 | Add distance and amplitude culling | S | 2 | Early exit for inaudible sources | P1 |
| 5.3 | Enforce voice limit (64 simultaneous) | S | 2 | Sort by priority, take top N | P1 |
| 5.4 | Optimize with spatial hashing | M | 4 | Fast neighbor queries for dense scenes | P2 |
| 5.5 | Test with 200+ sources in dense scene | M | 4 | Create test scene, verify <1ms culling time | P1 |

**Phase 5 Total:** 16 points

### Lean Analysis
- **Eliminate Waste:** Can we skip prioritization initially? NO - 200 sources will be muddy
- **Amplify Learning:** What's the real voice limit? Test at 5.5 to find out
- **Decide Late:** Can we defer spatial hashing (5.4)? YES if linear search is fast enough
- **Optimize Whole:** Does this work for both desktop and iOS? YES - same culling logic

### Phase 5 Recommendations
1. **Do 5.1-5.3 first** - core prioritization logic
2. **5.4 is optional optimization** - measure first, optimize if needed
3. **5.5 is GO/NO-GO gate** - if performance fails, revisit 5.4

---

## Phase 6: Debug Visualization

**Phase Goal:** Visual debugging of spatial audio sources
**Phase Size:** 16 points
**Dependencies:** Phase 5 complete (need full system working)
**Risk:** Low (tooling, not core functionality)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 6.1 | Implement gizmo rendering for active sources | M | 4 | Sphere gizmos with falloff ranges | P1 |
| 6.2 | Add color-coding by bus type | S | 2 | Visual differentiation of audio categories | P2 |
| 6.3 | Implement amplitude animation (brightness pulse) | S | 2 | Visual feedback for sound intensity | P2 |
| 6.4 | Add selection raycasting and inspector panel | M | 4 | Click source → show details in egui | P1 |
| 6.5 | Add occlusion ray visualization | S | 2 | Green = clear, red = occluded | P2 |
| 6.6 | Test on complex scene with 50+ sources | S | 2 | Verify visualization remains readable | P2 |

**Phase 6 Total:** 16 points

### Lean Analysis
- **Eliminate Waste:** Is visualization necessary? YES - critical for debugging spatial audio
- **Amplify Learning:** Will this reveal mix problems? YES - that's the purpose
- **Build Quality In:** Should this be P1? YES for 6.1 and 6.4, others are polish
- **Deliver Fast:** Can we ship minimal version? YES - 6.1 and 6.4 are essential, others are nice-to-have

### Phase 6 Recommendations
1. **Do 6.1 and 6.4 first** - core debug functionality
2. **6.2, 6.3, 6.5 are polish** - do when inspired
3. **This is a checkpoint** - use visualization to verify Phases 1-5 work correctly

---

## Phase 7: Mixer Panel

**Phase Goal:** Professional mixing console in egui
**Phase Size:** 18 points
**Dependencies:** Phase 4 complete (needs mixer state)
**Risk:** Low (UI work)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 7.1 | Implement egui mixer panel with channel strips | L | 8 | Layout 5 bus channels + master, faders, meters | P1 |
| 7.2 | Add EQ controls (3-band, collapsible) | M | 4 | Low shelf, mid bell, high shelf UI | P2 |
| 7.3 | Add solo/mute buttons | S | 2 | Isolation for debugging | P1 |
| 7.4 | Implement metering (peak/RMS from audio thread) | M | 4 | Lock-free meter reads, visual bars | P1 |
| 7.5 | Add LUFS integrated loudness meter | S | 2 | Master bus loudness monitoring | P3 |
| 7.6 | Implement preset save/load (JSON) | S | 2 | Serialize mixer state, version control | P2 |

**Phase 7 Total:** 22 points

### Lean Analysis
- **Eliminate Waste:** Do we need LUFS (7.5)? NO - defer to P3
- **Amplify Learning:** Will this improve mix quality? YES - professional tools = professional results
- **Build Quality In:** Is metering (7.4) essential? YES - you can't mix what you can't measure
- **Deliver Fast:** What's minimum viable mixer? 7.1, 7.3, 7.4

### Phase 7 Recommendations
1. **Do 7.1 first** - foundation for all other tasks
2. **7.3 and 7.4 immediately** - essential for mixing
3. **7.2 and 7.6 are P2** - important but not blocking
4. **7.5 is P3** - nice-to-have professional feature

---

## Phase 8: Soundscape Zones

**Phase Goal:** Layered ambient audio zones
**Phase Size:** 14 points
**Dependencies:** Phase 3 complete (needs component system)
**Risk:** Medium (complex activation logic)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 8.1 | Implement `SoundscapeZone` component | S | 2 | Define zone shapes, layers, fade distance | P1 |
| 8.2 | Add zone activation system (listener position) | M | 4 | Track listener, activate/deactivate zones | P1 |
| 8.3 | Implement crossfading between overlapping zones | M | 4 | Smooth transitions, prevent popping | P1 |
| 8.4 | Add randomized layer playback | S | 2 | Occasional sounds (birds, creaks) with random timing | P2 |
| 8.5 | Test with 10+ overlapping zones | S | 2 | Verify performance, smooth crossfades | P1 |

**Phase 8 Total:** 14 points

### Lean Analysis
- **Eliminate Waste:** Are zones necessary? YES - essential for hyper-dense soundscapes
- **Amplify Learning:** Will this reveal performance issues? YES - test at 8.5
- **Build Quality In:** Is crossfading (8.3) critical? YES - popping is unacceptable
- **Optimize Whole:** Does this work with prioritization (Phase 5)? YES - zones create sources that get prioritized

### Phase 8 Recommendations
1. **Do 8.1-8.3 sequentially** - core zone system
2. **8.4 is nice-to-have** - adds realism but not essential
3. **8.5 is verification** - test in realistic scenario

---

## Overall Scheduling Recommendations

### Critical Path (Sequential)
```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5
                        ↓
                  Phase 6 (can parallelize with Phase 7)
                        ↓
                  Phase 7 → Phase 8
Total: ~91 points
```

### Parallel Opportunities
- **During Phase 1:** Design Phase 3 component API
- **During Phase 5:** Build Phase 6 visualization (uses same source data)
- **After Phase 4:** Phases 6, 7, 8 can partially overlap (different subsystems)

### Risk Mitigation Strategy
1. **Phase 1.6 is a GO/NO-GO gate** - if basic playback fails, stop and debug
2. **Phase 2.4 spatial accuracy test** - verify HRTF works before proceeding
3. **Phase 3.5 concurrency test** - ensure lock-free sync works flawlessly
4. **Phase 5.5 performance test** - verify 200+ sources cull to 64 in <1ms
5. **Incremental commits** - don't batch entire phase into one PR

---

## WSJF Prioritization (P1 Tier)

Scoring against other P1 work:

| Item | Player Value | Time Criticality | Risk Reduction | CoD | Size | WSJF |
|------|--------------|------------------|----------------|-----|------|------|
| **Spatial Audio Epic** | 9 | 6 | 7 | 22 | 91 | **0.24** |
| Phase 1 alone | 3 | 4 | 8 | 15 | 14 | **1.07** |
| Phase 1-3 together | 8 | 5 | 8 | 21 | 52 | **0.40** |
| Phases 1-5 (core) | 9 | 6 | 8 | 23 | 83 | **0.28** |

**Interpretation:**
- **High player value** - spatial audio is core immersion feature
- **High time criticality** - needed for demos and content creation
- **High risk reduction** - vendoring eliminates dependency lag
- **Phases 1-3 are foundation** - nothing works without them
- **Phases 6-8 are tooling** - can defer slightly if needed

---

## Sequencing with Other Work

### Good to do BEFORE this epic:
- ✅ iOS deployment scripts (done)
- ✅ Basic ECS setup (done)
- Bevy rendering vendoring (provides debugging context)

### Good to do AFTER this epic:
- Agent ambient sounds (depends on spatial audio)
- Environmental soundscapes (depends on zones)
- Dialogue system (depends on Voice bus)
- Music system (depends on mixer)

### Can do IN PARALLEL:
- Content creation (3D assets, animations)
- Networking improvements (different subsystem)
- Game design prototyping (can use placeholder audio)

---

## Decision Points

### Before Starting Phase 1:
- [ ] Do we have bandwidth for ~3 months of audio work?
- [ ] Are there higher-priority P1 bugs blocking demos?
- [ ] Have we validated spatial audio is essential to Aspen vision?

### Before Starting Phase 2:
- [ ] Did Phase 1.6 smoke tests pass on both iOS and macOS?
- [ ] Do we understand Firewheel's lock-free guarantees?
- [ ] Is Steam Audio C++ library compatible with iOS?

### Before Starting Phase 3:
- [ ] Does binaural output sound correct? (Phase 2.4)
- [ ] Have we tested with headphones/earbuds on device?
- [ ] Do we understand the game thread → audio thread sync pattern?

### Before Starting Phase 4:
- [ ] Are moving sources glitch-free? (Phase 3.5)
- [ ] Have we tested with 10+ simultaneous sources?

### Before Starting Phase 5:
- [ ] Does bus routing work correctly? (Phase 4.5)
- [ ] Have we identified the voice limit threshold?

### Before Starting Phases 6-8 (Tooling):
- [ ] Is core spatial audio (Phases 1-5) solid?
- [ ] Have we tested on both iOS and macOS extensively?
- [ ] Can we demo spatial audio to stakeholders?

### Before Closing Epic:
- [ ] All platforms tested with 64+ voices?
- [ ] Spatial accuracy test passed (blindfolded pointing <15° error)?
- [ ] Mix quality validated by professional sound engineer?
- [ ] Performance test passed (<2ms audio thread on M1 iPad)?
- [ ] Documentation updated (API docs, audio design guidelines)?

---

## Minimum Viable Implementation

If we need to ship faster, the **minimum viable spatial audio** is:

**Phases 1-5 only** (83 points)
- Firewheel and Steam Audio integration
- ECS integration
- Bus mixer
- Prioritization

This provides:
- 3D positioned audio with HRTF
- Bus-based mixing
- Voice limiting for performance

**Deferred to later:**
- Debug visualization (Phase 6)
- Mixer panel UI (Phase 7)
- Soundscape zones (Phase 8)

**Trade-off:** Harder to debug and mix without tooling, but core spatial audio works.

---

## Summary

**Total Effort:** ~91 points (XXL epic)
**Confidence:** Medium (audio is complex, vendoring reduces some risk)
**Recommendation:** High priority - spatial audio is core to Aspen's immersion
**When to do it:** After basic rendering is stable, before content creation ramps up

This is essential for Aspen's "sense of place" design pillar. Unlike the Bevy renderer epic (P2 technical debt), spatial audio is P1 player-facing immersion.
