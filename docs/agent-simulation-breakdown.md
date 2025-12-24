# Agent Simulation Framework - Task Breakdown

**Epic:** Agent Simulation Framework (#5)
**Overall Size:** XXXL (165 points across 12 phases)
**Priority:** P0 (Critical - core differentiator for Marathon)

This document breaks down the 12 layers of the agent simulation framework into specific, sized tasks for prioritization and scheduling.

**Philosophy:** Generic composition over special cases. Emergence over authorship. Consequences without failure. Existence is cheap; cognition is expensive.

**Approach:** Incremental implementation. Each layer should be validated in isolation before adding the next. Start with foundation, add one layer at a time, continuously test performance and emergent behavior.

---

## Phase 1: Foundation (Entity Composition)

**Phase Goal:** Implement entity composition system with core component categories
**Phase Size:** 16 points
**Dependencies:** Bevy ECS
**Risk:** Low (building on solid ECS foundation)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 1.1 | Define entity component categories and marker traits | S | 2 | Create type hierarchy for components (Lifecycle, PhysicalPresence, Needs, etc.) | P0 |
| 1.2 | Implement Lifecycle state machine for all entity types | M | 4 | States: Unborn → Infant → Child → Adolescent → Adult → Elder → Deceased | P0 |
| 1.3 | Implement PhysicalPresence component with position and movement | M | 4 | Position, velocity, pathfinding integration, spatial indexing | P0 |
| 1.4 | Implement Needs system (biological and psychological drives) | M | 4 | Hunger, thirst, sleep, social, safety needs with decay and satisfaction | P0 |
| 1.5 | Implement Instincts system (species-typical behaviors) | S | 2 | Basic instincts (flee danger, seek food, seek shelter) | P0 |

**Phase 1 Total:** 16 points

### Lean Analysis
- **Eliminate Waste:** Can we use existing ECS patterns? YES - leverage Bevy's component model
- **Amplify Learning:** What will we learn? How to compose complex entities from simple components
- **Deliver Fast:** Can we implement incrementally? YES - add components one at a time
- **Build Quality In:** Risk of component coupling? YES - enforce clean boundaries between categories

### Phase 1 Recommendations
1. **Start with marker traits** - define component categories clearly
2. **Validate Lifecycle separately** - ensure state machine is solid before adding other components
3. **Test composition patterns** - verify entities can have any combination of components

---

## Phase 2: Life Arc System

**Phase Goal:** Implement life arc state machines and autonomy gradient
**Phase Size:** 12 points
**Dependencies:** Phase 1 complete
**Risk:** Medium (complex state transitions, schedule authority handoff)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 2.1 | Implement life arc state machines (Child → Adult → Elder) | L | 8 | Major state transitions with capability changes and schedule authority shifts | P0 |
| 2.2 | Implement autonomy gradient (None → Partial → Full) | S | 2 | Track who controls entity schedule (parent, self, institution) | P0 |
| 2.3 | Implement life arc transition triggers | S | 2 | Age-based, event-based, and condition-based transitions | P1 |

**Phase 2 Total:** 12 points

### Lean Analysis
- **Eliminate Waste:** Can we hardcode ages? NO - emergent transitions are key to feeling alive
- **Amplify Learning:** What will we learn? How autonomy affects entity behavior
- **Deliver Fast:** Can we implement incrementally? YES - start with simple age transitions
- **Build Quality In:** Risk of broken transitions? YES - comprehensive state transition testing

### Phase 2 Recommendations
1. **Start with simple age-based transitions** - Child(0-12) → Adolescent(13-17) → Adult(18+)
2. **Test authority handoff carefully** - ensure schedule control transfers correctly
3. **Validate edge cases** - what happens when parent dies while child is dependent?

---

## Phase 3: Schedule System

**Phase Goal:** Implement daily schedule generation from roles and personality
**Phase Size:** 14 points
**Dependencies:** Phase 2 complete
**Risk:** Medium (schedule conflicts, authority model complexity)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 3.1 | Implement daily schedule generation from roles | M | 4 | Generate time blocks (work, sleep, leisure) based on role and personality | P0 |
| 3.2 | Implement schedule authority model (autonomous, external, partial) | M | 4 | Track who can modify entity schedule (self, parent, employer) | P0 |
| 3.3 | Implement shared commitments between related entities | M | 4 | Couples, families, work teams share schedule blocks | P1 |
| 3.4 | Implement schedule regeneration on condition changes | S | 2 | Rebuild schedule when sick, pregnant, grieving, etc. | P1 |

**Phase 3 Total:** 14 points

### Lean Analysis
- **Eliminate Waste:** Can we use fixed schedules? NO - dynamic schedules enable emergent behavior
- **Amplify Learning:** What will we learn? How schedule conflicts get resolved
- **Deliver Fast:** Can we implement incrementally? YES - basic schedules first, then shared commitments
- **Build Quality In:** Risk of schedule conflicts? YES - need clear resolution rules

### Phase 3 Recommendations
1. **Start with single-entity schedules** - validate basic time blocking works
2. **Add authority later** - ensure autonomous schedules work before external control
3. **Test regeneration** - verify schedules adapt to condition changes

---

## Phase 4: Behavior Tree System

**Phase Goal:** Implement behavior tree framework and need-driven prioritization
**Phase Size:** 16 points
**Dependencies:** Phase 3 complete
**Risk:** High (performance critical, complex evaluation logic)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 4.1 | Implement behavior tree framework and evaluation engine | L | 8 | Tree structure, node types (sequence, selector, action), tick evaluation | P0 |
| 4.2 | Implement activity-specific behavior trees | M | 4 | Work, Leisure, Social, Maintenance activities with contextual behaviors | P0 |
| 4.3 | Implement need-driven behavior prioritization | M | 4 | Urgent needs interrupt schedule, utility-based selection | P0 |

**Phase 4 Total:** 16 points

### Lean Analysis
- **Eliminate Waste:** Can we use FSM instead? NO - behavior trees compose better
- **Amplify Learning:** What will we learn? How needs affect behavior selection
- **Deliver Fast:** Can we implement incrementally? YES - simple trees first, add complexity
- **Build Quality In:** Risk of infinite loops? YES - add evaluation limits and cycle detection

### Phase 4 Recommendations
1. **Start with simple trees** - single activity type, no interruption
2. **Profile evaluation performance** - this is frame-rate critical for nearby entities
3. **Test need interruption** - verify urgent hunger can interrupt work

---

## Phase 5: World Substrate

**Phase Goal:** Implement substrate layer types and environmental effects
**Phase Size:** 14 points
**Dependencies:** Phase 1 complete (can parallelize with Phases 2-4)
**Risk:** Medium (spatial queries performance, substance simulation)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 5.1 | Implement substrate layer types (substances, temperature, light, sound, air quality, contagion) | L | 8 | Spatial grids or continuous fields for environmental state | P1 |
| 5.2 | Implement trigger volume system for spatial events | S | 2 | Zones that affect entities (fire, poison gas, music) | P1 |
| 5.3 | Implement generic substance transfer and consumption | M | 4 | Entities consume/produce substances, transfer between locations | P2 |

**Phase 5 Total:** 14 points

### Lean Analysis
- **Eliminate Waste:** Can we skip substrate entirely? NO - enables fire, disease, pollution
- **Amplify Learning:** What will we learn? How spatial state affects entity behavior
- **Deliver Fast:** Can we implement incrementally? YES - start with simple trigger volumes
- **Build Quality In:** Risk of performance issues? YES - spatial queries must be efficient

### Phase 5 Recommendations
1. **Start with trigger volumes** - simpler than continuous fields
2. **Use spatial indexing** - KD-tree or grid for efficient queries
3. **Test with 500 entities** - verify performance scales

---

## Phase 6: Institutions

**Phase Goal:** Implement institution entity type with coordination blackboard
**Phase Size:** 16 points
**Dependencies:** Phase 3 complete
**Risk:** High (cross-entity coordination, emergent organizational behavior)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 6.1 | Implement Institution entity type with Governance, Resources, Reputation | M | 4 | Special entity type that manages members and shared state | P0 |
| 6.2 | Implement shared blackboard state for coordination | L | 8 | Shared memory for work assignments, inventory, schedules | P0 |
| 6.3 | Implement worker schedule template generation | M | 4 | Institutions create schedule templates for roles (baker, guard, teacher) | P1 |

**Phase 6 Total:** 16 points

### Lean Analysis
- **Eliminate Waste:** Can we hardcode workplace behavior? NO - emergent coordination is key
- **Amplify Learning:** What will we learn? How entities coordinate without communication
- **Deliver Fast:** Can we implement incrementally? YES - simple bakery first, then generalize
- **Build Quality In:** Risk of coordination deadlocks? YES - need clear state machine

### Phase 6 Recommendations
1. **Start with single institution** - bakery with 1 baker, 1 apprentice
2. **Validate blackboard pattern** - ensure shared state enables coordination
3. **Test with multiple institutions** - verify they don't interfere

---

## Phase 7: Relationships

**Phase Goal:** Implement relationship state and coordination levels
**Phase Size:** 14 points
**Dependencies:** Phase 3 complete
**Risk:** Medium (relationship evolution, coordination complexity)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 7.1 | Implement Relationship state and evolution | M | 4 | Bond strength, interaction history, sentiment tracking | P1 |
| 7.2 | Implement bond types (Romantic, Familial, Friendship, Professional, Caretaking) | M | 4 | Different bond types enable different interactions | P1 |
| 7.3 | Implement coordination levels (None → AdHoc → Recurring → Cohabiting → Dependent) | M | 4 | Stronger bonds enable tighter schedule coordination | P1 |
| 7.4 | Implement relationship effects on all simulation layers | S | 2 | Relationships affect needs, schedules, behaviors, conditions | P2 |

**Phase 7 Total:** 14 points

### Lean Analysis
- **Eliminate Waste:** Can we skip relationships? NO - core to "living village" feel
- **Amplify Learning:** What will we learn? How bonds enable coordination
- **Deliver Fast:** Can we implement incrementally? YES - family bonds first, then friends
- **Build Quality In:** Risk of relationship graph bugs? YES - need clear formation/dissolution rules

### Phase 7 Recommendations
1. **Start with familial bonds** - parent-child relationships are simplest
2. **Test coordination levels** - verify cohabiting couples share schedules
3. **Validate evolution** - bonds should strengthen/weaken from interactions

---

## Phase 8: Conditions

**Phase Goal:** Implement condition system with effects on all layers
**Phase Size:** 12 points
**Dependencies:** Phases 1-4 complete
**Risk:** Medium (condition interactions, contagion simulation)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 8.1 | Implement Condition system with severity and duration | M | 4 | Temporary states (sick, pregnant, grieving, drunk) | P1 |
| 8.2 | Implement condition effects (need modifiers, capability modifiers, schedule overrides, behavior modifiers) | M | 4 | Conditions affect all simulation layers | P1 |
| 8.3 | Implement contagion system for spreading conditions | M | 4 | Diseases spread through proximity and substrate | P2 |

**Phase 8 Total:** 12 points

### Lean Analysis
- **Eliminate Waste:** Can we hardcode illness? NO - generic conditions enable emergent drama
- **Amplify Learning:** What will we learn? How conditions cascade through simulation
- **Deliver Fast:** Can we implement incrementally? YES - simple illness first, then contagion
- **Build Quality In:** Risk of runaway epidemics? YES - need disease progression model

### Phase 8 Recommendations
1. **Start with non-contagious conditions** - pregnancy, grief, intoxication
2. **Add simple illness** - validate need modifiers and schedule overrides work
3. **Test contagion carefully** - ensure diseases don't wipe out entire village

---

## Phase 9: Tools & Capabilities

**Phase Goal:** Implement capability types and tool operator modes
**Phase Size:** 12 points
**Dependencies:** Phase 1 complete
**Risk:** Medium (capability composition, tool quality effects)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 9.1 | Implement capability types (Movement, Communication, Transformation, Force, Storage) | M | 4 | Abstract capabilities that entities and tools provide | P1 |
| 9.2 | Implement tool operator modes (Inhabit, Wield, Station, Wear) | M | 4 | Different ways entities use tools (house, hammer, oven, clothes) | P1 |
| 9.3 | Implement tools as institutional resources | S | 2 | Bakery owns ovens, blacksmith owns anvils | P2 |
| 9.4 | Implement tool quality affecting output | S | 2 | Better tools produce better results | P2 |

**Phase 9 Total:** 12 points

### Lean Analysis
- **Eliminate Waste:** Can we skip tools? NO - needed for production workflows
- **Amplify Learning:** What will we learn? How capabilities compose
- **Deliver Fast:** Can we implement incrementally? YES - simple wielded tools first
- **Build Quality In:** Risk of capability conflicts? YES - need clear capability stacking rules

### Phase 9 Recommendations
1. **Start with wielded tools** - hammer, axe, hoe
2. **Add stations** - ovens, forges, looms require entity presence
3. **Test quality modifiers** - verify master blacksmith + good anvil = quality sword

---

## Phase 10: Production Workflows

**Phase Goal:** Implement batch entity system for multi-step processes
**Phase Size:** 14 points
**Dependencies:** Phase 9 complete
**Risk:** High (workflow state persistence, interruption handling)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 10.1 | Implement batch entity system for multi-step processes | L | 8 | Bread batch progresses: mix → knead → proof → bake → cool | P1 |
| 10.2 | Implement recipe and phase system | M | 4 | Recipes define inputs, phases, outputs, time/skill requirements | P1 |
| 10.3 | Implement interruptible work with state persistence | S | 2 | Baker can pause kneading to serve customer, resume later | P2 |

**Phase 10 Total:** 14 points

### Lean Analysis
- **Eliminate Waste:** Can we use instant crafting? NO - multi-step processes enable depth
- **Amplify Learning:** What will we learn? How to persist complex workflow state
- **Deliver Fast:** Can we implement incrementally? YES - simple 1-phase recipes first
- **Build Quality In:** Risk of lost state on interruption? YES - need save/restore

### Phase 10 Recommendations
1. **Start with simple recipes** - bread (mix → bake), no interruption
2. **Add multi-phase** - validate state transitions work
3. **Test interruption** - ensure baker can resume kneading after break

---

## Phase 11: Performance Architecture

**Phase Goal:** Implement simulation tier system and discrete event simulation
**Phase Size:** 15 points
**Dependencies:** Phase 4 complete
**Risk:** Critical (performance determines feasibility of 500 entities)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 11.1 | Implement simulation tier system (Attached, Nearby, Background, Wildlife) | L | 8 | Different update strategies for different proximity levels | P0 |
| 11.2 | Implement tier promotion/demotion based on player proximity | M | 4 | Seamless transitions as players move through world | P0 |
| 11.3 | Implement discrete event simulation and wake queue | S | 3 | Background entities only update when something changes | P1 |

**Phase 11 Total:** 15 points

### Lean Analysis
- **Eliminate Waste:** Can we update all entities every frame? NO - exceeds iPad budget
- **Amplify Learning:** What will we learn? How to scale simulation efficiently
- **Deliver Fast:** Can we implement incrementally? YES - simple tier system first
- **Build Quality In:** Risk of visible tier transitions? YES - need smooth fade

### Phase 11 Recommendations
1. **Profile early and often** - this determines if 500 entities is feasible
2. **Start with 2 tiers** - nearby (full), background (interpolated)
3. **Add wake queue** - discrete events are key to efficient background simulation
4. **Test on iPad** - don't optimize for desktop and hope it works

---

## Phase 12: Networking & Sync

**Phase Goal:** Implement CRDT-based synchronization and entity ownership
**Phase Size:** 16 points
**Dependencies:** All previous phases complete
**Risk:** Critical (desyncs break multiplayer experience)

### Tasks

| # | Task | Size | Points | Rationale | Priority |
|---|------|------|-----------|----------|----------|
| 12.1 | Implement CRDT-based state synchronization for slow-changing layers | L | 8 | Life arcs, schedules, relationships use CRDTs for eventual consistency | P0 |
| 12.2 | Implement behavior tree outcome synchronization | M | 4 | Fast-changing behavior trees sync outcomes, not state | P0 |
| 12.3 | Implement entity ownership model | S | 2 | Entities owned by nearest player, ownership transfers on proximity | P0 |
| 12.4 | Implement spatial ownership for substrate | S | 2 | World divided into chunks, each owned by one peer | P1 |

**Phase 12 Total:** 16 points

### Lean Analysis
- **Eliminate Waste:** Can we use lockstep sync? NO - too slow for 500 entities
- **Amplify Learning:** What will we learn? How to sync without desyncs
- **Deliver Fast:** Can we implement incrementally? YES - basic ownership first
- **Build Quality In:** Risk of desyncs? YES - extensive multiplayer testing required

### Phase 12 Recommendations
1. **Start with entity ownership** - simplest sync model
2. **Add CRDT sync for schedules** - slow-changing state is easier
3. **Test with 2 peers first** - validate before scaling to 4
4. **Add reconciliation** - handle ownership conflicts gracefully
5. **Stress test with 500 entities** - verify no desyncs in 1-hour sessions

---

## Overall WSJF Analysis

Using Cost of Delay / Duration to prioritize phases:

| Phase | Size | Business Value | Time Criticality | Risk Reduction | CoD | WSJF |
|-------|------|----------------|------------------|----------------|-----|------|
| Phase 1 | 16 | 8/10 | 9/10 | 9/10 | 26 | 1.63 |
| Phase 2 | 12 | 7/10 | 8/10 | 8/10 | 23 | 1.92 |
| Phase 3 | 14 | 8/10 | 8/10 | 7/10 | 23 | 1.64 |
| Phase 4 | 16 | 9/10 | 9/10 | 9/10 | 27 | 1.69 |
| Phase 11 | 15 | 10/10 | 10/10 | 10/10 | 30 | 2.00 |
| Phase 12 | 16 | 10/10 | 10/10 | 10/10 | 30 | 1.88 |

**Recommendation:** Implement in order 1 → 2 → 3 → 4 → 11 (critical path for viability), then parallelize remaining layers.

## Success Metrics

### Performance Targets
- 500 entities simulated simultaneously
- 60 FPS on iPad Pro with full simulation load
- Background entities consume <5% of frame budget
- Tier transitions are visually seamless

### Multiplayer Targets
- 2-4 peers supported
- <100ms sync latency for state propagation
- Zero desyncs in 1-hour multiplayer sessions
- Ownership transitions are seamless

### Emergence Targets
- Entities follow believable daily routines without hand-authoring
- Relationships form and evolve through interaction
- Institutions coordinate workers without explicit communication
- Unexpected behaviors emerge from generic rule interactions

## Implementation Notes

1. **Incremental Development**: Each phase should be fully validated before moving to the next
2. **Performance First**: Profile on iPad hardware from Phase 1 onwards
3. **Test Emergent Behavior**: Create test scenarios that exercise rule interactions
4. **Document Invariants**: Clear rules for how systems interact prevents bugs
5. **Content Light**: Focus on generic rules that recombine, not hand-authored content
6. **Multiplayer Always**: Test sync from Phase 1, don't bolt on later

## Total Breakdown

- **Total Points:** 165
- **Total Phases:** 12
- **Critical Path:** Phases 1, 2, 3, 4, 11, 12 (89 points)
- **Parallel Work:** Phases 5-10 can be done alongside or after critical path (76 points)
