---
title: "RFCs"
description: "Request for Comments (RFCs) for major design decisions in the Lonni project."
updated_at: "2026-07-28"
---

# RFCs

Request for Comments (RFCs) for major design decisions in the Lonni project.

## Active RFCs

- [RFC 0001: CRDT Synchronization Protocol over iroh-gossip](./0001-crdt-gossip-sync.md) - Draft

## RFC Process

1. **Draft**: Initial proposal, open for discussion
2. **Review**: Team reviews and provides feedback
3. **Accepted**: Approved for implementation
4. **Implemented**: Design has been built
5. **Superseded**: Replaced by a newer RFC

RFCs are living documents - they can be updated as we learn during implementation.

## When to Write an RFC

Write an RFC when:
- Making architectural decisions that affect multiple parts of the system
- Choosing between significantly different approaches
- Introducing new protocols or APIs
- Making breaking changes

Don't write an RFC for:
- Small bug fixes
- Minor refactors
- Isolated feature additions
- Experimental prototypes

## RFC Format

- **Narrative first**: Tell the story of why and how
- **Explain trade-offs**: What alternatives were considered?
- **API examples**: Show how it would be used (not full implementations)
- **Open questions**: What's still unclear?
- **Success criteria**: How do we know it works?