# Serialization Policy

**Never use serde for serialization in this project.**

We use `rkyv` exclusively for all serialization needs:
- Network messages
- Component synchronization
- Persistence
- Any data serialization

If a type from a dependency (like Bevy) doesn't support rkyv, we vendor it and add the rkyv derives ourselves.
