---
title: "Estimation and Prioritization"
description: "How r3t Studios sizes, estimates, and prioritizes work, grounded in Lean Software Development principles adapted for indie game development."
updated_at: "2026-07-28"
---

# Estimation and Prioritization

This document defines how r3t Studios sizes, estimates, and prioritizes work. Our approach is grounded in **Lean Software Development** principles, adapted for indie game development.

---

## Lean Principles as Evaluation Questions

Every sizing and prioritization decision should be evaluated through these questions:

| Principle | Evaluation Questions |
|-----------|---------------------|
| **Eliminate Waste** | Does this reduce waste? Is this the simplest solution? Are we building something nobody needs? |
| **Amplify Learning** | What will we learn from this? Does this reduce uncertainty? Should we prototype first? |
| **Decide as Late as Possible** | Do we have enough information to commit? Can this decision be deferred responsibly? |
| **Deliver as Fast as Possible** | What's the smallest increment that delivers value? What's blocking flow? |
| **Empower the Team** | Do I have what I need to do this? Who else should be involved in this decision? |
| **Build Integrity In** | Are we building quality in from the start? Will this create technical debt? |
| **Optimize the Whole** | Does this improve the whole system or just one part? Are we sub-optimizing? |

---

## Sizing

We use **powers of 2** for effort estimation: `1, 2, 4, 8, 16, 32`

This scale is intentionally coarse. Precision in estimation is waste—we optimize for fast, good-enough decisions.

| Size | Points | Description |
|------|--------|-------------|
| **XS** | 1 | Trivial change. A few lines, minimal risk. Less than 2 hours. |
| **S** | 2 | Small task. Well-understood, limited scope. Half a day. |
| **M** | 4 | Medium task. Some complexity or unknowns. About 1 day. |
| **L** | 8 | Large task. Multiple components or significant complexity. 2-3 days. |
| **XL** | 16 | Very large. Should probably be an epic. ~1 week. |
| **XXL** | 32 | Epic-scale work. Multi-phase refactoring or major features. 2-4 weeks. |

### Sizing Guidelines

**Before assigning a size, ask:**

- *Eliminate Waste*: Is this the smallest scope that delivers value?
- *Deliver Fast*: Can we break this down further?
- *Amplify Learning*: Is there uncertainty that inflates the estimate? Should we prototype/spike first?
- *Optimize the Whole*: Does the estimate account for integration, testing, and platform testing (iOS + macOS)?

**If the size is XL (16) or larger:**

This is epic territory. Break it into phases or sub-tasks. An XL/XXL indicates the work is not well-understood or needs careful sequencing. Consider writing an RFC for XXL work.

---

## Priority

Priority is based on **Cost of Delay**—the impact of not doing this work now.

| Label | Priority | Cost of Delay Profile | Description |
|-------|----------|----------------------|-------------|
| **P0** | Critical | Expedite | Game-breaking bug, crash on launch, data loss, or blocks release. Drop everything. Every hour matters. |
| **P1** | High | Fixed Date / High Value | Release blocker, community-visible bug, or deadline-driven (demo, event, content creator access). Do this sprint. |
| **P2** | Medium | Standard | Normal feature work, engine improvements, content additions. Standard backlog work—prioritize by WSJF. |
| **P3** | Low | Intangible | Nice-to-have, polish, research, experiments. Do when capacity allows or inspiration strikes. |

### Priority Guidelines

**Before assigning a priority, ask:**

- *Eliminate Waste*: Should we do this at all? What happens if we never do it?
- *Amplify Learning*: Does doing this first reduce uncertainty for other work?
- *Decide Late*: Do we need to commit now, or can we learn more first? (Especially important for game design decisions)
- *Optimize the Whole*: Does this unblock other high-value work? Does it improve both engine (Marathon) and game (Aspen)?

---

## WSJF (Weighted Shortest Job First)

For backlog ordering within a priority tier, we use WSJF:

```
WSJF = Cost of Delay / Job Size
```

Where **Cost of Delay** considers:

- **Player Value**: Does this improve player experience, immersion, or enjoyment?
- **Time Criticality**: Content creator previews, demo deadlines, platform submission windows
- **Risk Reduction**: Tech debt payoff, engine stability, enables future content/features

Higher WSJF = do it first.

### Example (Game Dev Context)

| Item | Player Value | Time Criticality | Risk Reduction | CoD | Size | WSJF |
|------|--------------|------------------|----------------|-----|------|------|
| Spatial Audio | 8 | 6 | 4 | 18 | 8 | 2.25 |
| Agent AI Polish | 6 | 2 | 2 | 10 | 16 | 0.63 |
| Fix iOS Crash | 10 | 10 | 8 | 28 | 4 | 7.0 |
| Low-Poly Tree Assets | 4 | 2 | 1 | 7 | 2 | 3.5 |

**Order: iOS Crash (7.0), Tree Assets (3.5), Spatial Audio (2.25), AI Polish (0.63)**

The crash gets fixed first (high value, small effort), then quick content wins, then larger features.

---

## Area Labels and Work Types

Our `area/` labels map to different types of work:

| Area | Typical Work | Considerations |
|------|--------------|----------------|
| **area/core** | Engine foundation | High risk—test thoroughly, affects everything |
| **area/rendering** | Graphics, PBR, cameras | Test on iOS + macOS, check DPI scaling |
| **area/audio** | Spatial audio, sound | Requires device testing, earbuds vs speakers |
| **area/networking** | P2P, CRDT sync | Test with poor connections, edge cases |
| **area/platform** | iOS/macOS specific | Platform-specific testing required |
| **area/simulation** | Agent behaviors, AI | Needs playtesting, emergent behavior testing |
| **area/content** | Art, audio assets, dialogue | Artistic review, asset pipeline testing |
| **area/ui-ux** | Menus, HUD, accessibility | Needs user testing, various screen sizes |
| **area/tooling** | Build, CI/CD, dev tools | Verify on fresh checkout |
| **area/docs** | Technical specs, RFCs | Keep updated as code evolves |
| **area/infrastructure** | Deployment, hosting | Test in staging before production |
| **area/rfc** | Design proposals | Gather feedback before committing |

---

## Quick Reference

### When Sizing

1. Compare to recent similar work
2. Use the Lean questions to validate scope
3. If XL or larger, consider making it an epic with sub-tasks
4. Bias toward smaller—deliver fast, learn, iterate
5. Remember: estimates are for ordering work, not commitments

### When Prioritizing

1. Is it game-breaking (crash/data loss)? If yes, **P0**—do it now
2. Is there a deadline (demo/release)? If yes, **P1**—this sprint
3. Otherwise, score WSJF and order the backlog as **P2**
4. Nice-to-haves and experiments go to **P3**
5. Re-evaluate priorities weekly—inspiration and context change

### For Solo Work

- **Batching**: Group similar work (all rendering tasks, all iOS fixes) to minimize context switching
- **Flow**: Protect deep work time—don't interrupt XL tasks with small P3 items
- **Momentum**: Sometimes doing a quick XS/S task builds momentum for harder work
- **Inspiration**: If you're excited about a P3 item, that's valuable—creative energy matters in game dev

---

## References

- Poppendieck, Mary & Tom. *Lean Software Development: An Agile Toolkit*. Addison-Wesley, 2003.
- Reinertsen, Donald G. *The Principles of Product Development Flow*. Celeritas Publishing, 2009.
- [Cost of Delay - Black Swan Farming](https://blackswanfarming.com/cost-of-delay/)
- [WSJF - Scaled Agile Framework](https://framework.scaledagile.com/wsjf/)
