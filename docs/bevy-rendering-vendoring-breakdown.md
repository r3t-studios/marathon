# Bevy Rendering Vendoring - Task Breakdown

**Epic:** Vendor Bevy Renderer and Eliminate Window Component Duplication (#2)
**Overall Size:** XXL (32+ points across 5 phases)
**Priority:** P2 (Medium - architectural improvement)

This document breaks down the 5 phases into specific, sized tasks for prioritization and scheduling.

---

## Phase 1: Vendoring

**Phase Goal:** Bring Bevy's rendering stack into Marathon codebase
**Phase Size:** 20 points
**Dependencies:** None (can start immediately)
**Risk:** Medium (large code drop, potential API mismatches)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 1.1 | Vendor `bevy_render` core into `crates/libmarathon/src/render/` | L | 8 | ~15K LOC, complex module structure, need to preserve API surface | P2 |
| 1.2 | Vendor `bevy_pbr` materials and lighting | M | 4 | Smaller than render core, well-isolated system | P2 |
| 1.3 | Vendor `bevy_core_pipeline` | S | 2 | Thin abstraction layer over render core | P2 |
| 1.4 | Vendor wgpu integration helpers | S | 2 | Limited surface area, mostly type wrappers | P2 |
| 1.5 | Update `Cargo.toml` and remove Bevy renderer dependencies | XS | 1 | Straightforward dependency changes | P2 |
| 1.6 | Verify existing rendering still works (smoke test) | M | 4 | Need to test all platforms, lighting, PBR materials | P1 |

**Phase 1 Total:** 21 points

### Lean Analysis
- **Eliminate Waste:** Is vendoring 15K+ LOC necessary? YES - window state duplication is causing bugs
- **Amplify Learning:** What will we learn? How deeply Bevy's renderer couples to Window components
- **Deliver Fast:** Can we vendor incrementally? YES - by module (render, then pbr, then pipeline)
- **Build Quality In:** Risk of introducing regressions? YES - comprehensive smoke testing critical

### Phase 1 Recommendations
1. **Do 1.1-1.5 as a batch** - vendoring is all-or-nothing, partial state is worse
2. **Do 1.6 immediately after** - verify nothing broke before proceeding
3. **Consider:** Create a feature flag `vendored-renderer` to toggle between vendored/upstream during transition

---

## Phase 2: Renderer Refactoring

**Phase Goal:** Make renderer work with winit handles directly
**Phase Size:** 18 points
**Dependencies:** Phase 1 complete
**Risk:** High (core renderer architecture changes)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 2.1 | Design `WindowInfo` abstraction for render queries | S | 2 | Clear API, minimal state | P1 |
| 2.2 | Modify renderer initialization to accept winit handles | M | 4 | Need to trace through render graph setup | P1 |
| 2.3 | Update `RawHandleWrapper` to provide window info | S | 2 | Add methods for size, scale_factor queries | P2 |
| 2.4 | Refactor camera viewport calculations | M | 4 | Cameras need aspect ratio, DPI - multiple call sites | P1 |
| 2.5 | Audit and update all window queries in render systems | L | 8 | Many systems query window, need comprehensive search | P1 |
| 2.6 | Verify PBR materials work with new architecture | M | 4 | Test metallic/roughness, normal maps, AO | P1 |

**Phase 2 Total:** 24 points

### Lean Analysis
- **Eliminate Waste:** Can we avoid refactoring everything? NO - window queries are scattered
- **Amplify Learning:** Should we prototype WindowInfo first? YES - design task 2.1 is critical
- **Decide Late:** Can we defer PBR verification? NO - it's core to the aesthetic
- **Optimize Whole:** Does this improve both desktop and iOS? YES - fixes DPI bugs on both

### Critical Path
```
2.1 (WindowInfo design)
  ↓
2.2 (renderer init) → 2.3 (RawHandleWrapper)
  ↓
2.4 (cameras) + 2.5 (audit systems)
  ↓
2.6 (PBR verification)
```

### Phase 2 Recommendations
1. **Start with 2.1** - get design right before touching renderer
2. **Parallelize 2.4 and 2.5** - different areas of codebase
3. **Consider:** Keep old Window component code paths behind feature flag during transition

---

## Phase 3: Executor Cleanup

**Phase Goal:** Remove duplicate Bevy Window components
**Phase Size:** 8 points
**Dependencies:** Phase 2 complete (renderer no longer needs Window components)
**Risk:** Low (pure deletion once renderer is independent)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 3.1 | Remove `bevy::window::Window` creation from iOS executor | S | 2 | Delete code, verify iOS still builds | P1 |
| 3.2 | Remove `bevy::window::Window` creation from desktop executor | S | 2 | Delete code, verify desktop still works | P1 |
| 3.3 | Migrate window config to winit `WindowAttributes` | M | 4 | Some logic may have lived in Bevy window creation | P2 |
| 3.4 | Remove `WindowMode` enum usage | XS | 1 | Straightforward deletion | P2 |
| 3.5 | Clean up unused imports and dead code | XS | 1 | Cargo clippy + manual review | P3 |

**Phase 3 Total:** 10 points

### Lean Analysis
- **Eliminate Waste:** This entire phase IS waste elimination - removing duplicate state
- **Deliver Fast:** Can we do this immediately after Phase 2? YES - it's pure cleanup
- **Build Quality In:** Risk of breaking something? LOW if Phase 2 is solid

### Phase 3 Recommendations
1. **Do 3.1 and 3.2 together** - both platforms should behave identically
2. **Do 3.5 last** - easy win after harder work
3. **Fast phase:** Mostly verification

---

## Phase 4: egui Integration

**Phase Goal:** Ensure debug UI works with winit-only window state
**Phase Size:** 6 points
**Dependencies:** Phase 3 complete
**Risk:** Low (egui is already vendored and working)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 4.1 | Update debug UI to query scale factor from winit | S | 2 | Replace any Bevy window queries | P1 |
| 4.2 | Verify custom input system still works | S | 2 | Input already uses custom event buffer | P1 |
| 4.3 | Test DPI scaling on HiDPI displays | S | 2 | Manual testing on Retina macOS + iPad | P1 |
| 4.4 | Update debug UI documentation | XS | 1 | Reflect new architecture | P3 |

**Phase 4 Total:** 7 points

### Lean Analysis
- **Amplify Learning:** Will this reveal DPI bugs? YES - explicit test for it
- **Build Quality In:** Test HiDPI early? YES - that's what this phase is

### Phase 4 Recommendations
1. **Do 4.1-4.3 as quick verification** - egui should "just work" since we already vendored it
2. **This is a checkpoint** - if 4.3 reveals DPI issues, they're from Phase 2/3

---

## Phase 5: Testing & Documentation

**Phase Goal:** Comprehensive verification and knowledge capture
**Phase Size:** 12 points
**Dependencies:** Phases 1-4 complete
**Risk:** Low (pure verification)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|--------|-----------|----------|
| 5.1 | PBR materials test with low-poly assets | M | 4 | Create test scene, verify metallic/roughness | P1 |
| 5.2 | Lighting system verification | M | 4 | Point, directional, spot lights + shadows | P1 |
| 5.3 | Cross-platform testing battery | S | 2 | macOS desktop, macOS Retina, iOS device, iPad simulator | P1 |
| 5.4 | Update architecture docs (RFC or new doc) | S | 2 | Explain window ownership, renderer changes | P2 |
| 5.5 | Remove obsolete TODOs and comments | XS | 1 | Code archaeology, cleanup | P3 |
| 5.6 | Create before/after architecture diagrams | S | 2 | Visual explanation for future contributors | P3 |

**Phase 5 Total:** 15 points

### Lean Analysis
- **Amplify Learning:** Testing amplifies confidence, docs amplify knowledge transfer
- **Build Quality In:** When should we test? CONTINUOUSLY, but this is final verification
- **Eliminate Waste:** Are diagrams worth 2 points? YES if they prevent future confusion

### Phase 5 Recommendations
1. **Do 5.1-5.3 first** - verification before celebration
2. **Do 5.4 immediately** - knowledge is fresh
3. **Do 5.5-5.6 when inspired** - nice-to-haves, P3 priority

---

## Overall Scheduling Recommendations

### Critical Path (Sequential)
```
Phase 1 → Phase 2 → Phase 3 → Phase 4 → Phase 5
Total: ~77 points
```

### Parallel Opportunities
- **During Phase 1:** Write Phase 2 design docs (2.1)
- **During Phase 2:** Plan Phase 3 deletions
- **During Phase 5:** Parallelize testing (5.1, 5.2, 5.3) if multiple devices available

### Risk Mitigation Strategy
1. **Phase 1.6 is a GO/NO-GO gate** - if smoke tests fail, stop and debug
2. **Phase 2.1 design review** - get feedback on WindowInfo before implementing
3. **Feature flags** - keep ability to toggle between old/new during Phases 1-3
4. **Incremental commits** - don't batch entire phase into one PR

---

## WSJF Prioritization (Within P2 Tier)

Scoring against other P2 work (hypothetical):

| Item | Player Value | Time Criticality | Risk Reduction | CoD | Size | WSJF |
|------|--------------|------------------|----------------|-----|------|------|
| **Bevy Vendor Epic** | 4 | 2 | 8 | 14 | 32 | **0.44** |
| Phase 1 alone | 3 | 2 | 9 | 14 | 21 | **0.67** |
| Phase 2 alone | 6 | 3 | 9 | 18 | 24 | **0.75** |

**Interpretation:**
- **Low player value initially** - this is technical debt, not features
- **High risk reduction** - fixes DPI bugs, enables future renderer work
- **Do Phase 1 + 2 together** - they're meaningless separately
- **Compare to:** Agent simulation (epic #5) likely has WSJF > 1.0, do that first if capacity allows

---

## Sequencing with Other Work

### Good to do BEFORE this epic:
- ✅ iOS deployment scripts (done)
- ✅ Basic ECS setup (done)
- Any small P1 bugs or quick wins

### Good to do AFTER this epic:
- Advanced rendering features (bloom, post-processing)
- Agent simulation rendering (needs clean renderer)
- Spatial audio visualization (uses renderer)

### Can do IN PARALLEL:
- Networking improvements (different subsystem)
- Content creation (doesn't depend on window architecture)
- Game design prototyping

---

## Decision Points

### Before Starting Phase 1:
- [ ] Do we have bandwidth for ~2 months of rendering work?
- [ ] Are there higher-priority bugs blocking players/demos?
- [ ] Have we validated PBR aesthetic matches Aspen vision?

### Before Starting Phase 2:
- [ ] Did Phase 1.6 smoke tests pass?
- [ ] Do we understand all Window component usage?
- [ ] Is WindowInfo design reviewed and approved?

### Before Starting Phase 3:
- [ ] Does renderer work 100% without Window components?
- [ ] Have we tested on both iOS and desktop?

### Before Closing Epic:
- [ ] All platforms tested with HiDPI/Retina displays?
- [ ] PBR materials look correct with low-poly assets?
- [ ] Architecture docs updated?
- [ ] Can we confidently say "winit is single source of truth"?

---

## Summary

**Total Effort:** ~77 points (XXL epic)
**Confidence:** Medium (vendoring is well-understood, refactoring has unknowns)
**Recommendation:** Defer until higher-value work (agent simulation, core gameplay) is stable
**When to do it:** When DPI bugs become P1, or when we need renderer extensibility

This is important technical debt payoff but not immediately urgent. The current duplicate window state works, just inelegantly.
