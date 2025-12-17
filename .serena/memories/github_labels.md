# GitHub Labels

This file contains the standard label configuration for r3t-studios repositories.

## Labels from marathon repository

These labels are currently defined in the marathon repository:

### Area Labels

| Name | Color | Description |
|------|-------|-------------|
| `area/core` | `#0052CC` | Foundation systems, memory management, math libraries, data structures, core utilities |
| `area/rendering` | `#0E8A16` | Graphics pipeline, Bevy rendering, shaders, materials, lighting, cameras, meshes, textures |
| `area/audio` | `#1D76DB` | Spatial audio engine, sound playback, audio mixing, music systems, 3D audio positioning |
| `area/networking` | `#5319E7` | iroh P2P, CRDT sync, gossip protocol, network replication, connection management |
| `area/platform` | `#0075CA` | iOS/macOS platform code, cross-platform abstractions, input handling, OS integration |
| `area/simulation` | `#FBCA04` | Agent systems, NPC behaviors, AI, game mechanics, interactions, simulation logic |
| `area/content` | `#C5DEF5` | Art assets, models, textures, audio files, dialogue trees, narrative content, game data |
| `area/ui-ux` | `#D4C5F9` | User interface, menus, HUD elements, input feedback, screen layouts, navigation |
| `area/tooling` | `#D93F0B` | Build systems, CI/CD pipelines, development tools, code generation, testing infrastructure |
| `area/docs` | `#0075CA` | Documentation, technical specs, RFCs, architecture decisions, API docs, tutorials, guides |
| `area/infrastructure` | `#E99695` | Deployment pipelines, hosting, cloud services, monitoring, logging, DevOps, releases |
| `area/rfc` | `#FEF2C0` | RFC proposals, design discussions, architecture planning, feature specifications |

## Labels referenced in issue templates but not yet created

The following labels are referenced in issue templates but don't exist in the repository yet:

| Name | Used In | Suggested Color | Description |
|------|---------|-----------------|-------------|
| `epic` | epic.yml | `#3E4B9E` | Large body of work spanning multiple features |

## Command to create all labels in a new repository

```bash
# Area labels
gh label create "area/core" --description "Foundation systems, memory management, math libraries, data structures, core utilities" --color "0052CC"
gh label create "area/rendering" --description "Graphics pipeline, Bevy rendering, shaders, materials, lighting, cameras, meshes, textures" --color "0E8A16"
gh label create "area/audio" --description "Spatial audio engine, sound playback, audio mixing, music systems, 3D audio positioning" --color "1D76DB"
gh label create "area/networking" --description "iroh P2P, CRDT sync, gossip protocol, network replication, connection management" --color "5319E7"
gh label create "area/platform" --description "iOS/macOS platform code, cross-platform abstractions, input handling, OS integration" --color "0075CA"
gh label create "area/simulation" --description "Agent systems, NPC behaviors, AI, game mechanics, interactions, simulation logic" --color "FBCA04"
gh label create "area/content" --description "Art assets, models, textures, audio files, dialogue trees, narrative content, game data" --color "C5DEF5"
gh label create "area/ui-ux" --description "User interface, menus, HUD elements, input feedback, screen layouts, navigation" --color "D4C5F9"
gh label create "area/tooling" --description "Build systems, CI/CD pipelines, development tools, code generation, testing infrastructure" --color "D93F0B"
gh label create "area/docs" --description "Documentation, technical specs, RFCs, architecture decisions, API docs, tutorials, guides" --color "0075CA"
gh label create "area/infrastructure" --description "Deployment pipelines, hosting, cloud services, monitoring, logging, DevOps, releases" --color "E99695"
gh label create "area/rfc" --description "RFC proposals, design discussions, architecture planning, feature specifications" --color "FEF2C0"

# Issue type labels
gh label create "epic" --description "Large body of work spanning multiple features" --color "3E4B9E"
```

## Notes

- The marathon repository has 12 labels defined, all with the `area/` prefix
- The `epic` label is referenced in the epic.yml issue template but hasn't been created yet in either marathon or aspen
- All area labels use distinct colors for easy visual identification
