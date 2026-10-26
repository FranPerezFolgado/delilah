# Security & Completeness Checklist: Mobile access analysis

**Purpose**: Unit tests for the spec's requirements on the analysis: options comparison, iPhone limits, security model, work-Mac fallback, scope (including launching work), roadmap and docs updates.
**Created**: 2026-10-26
**Feature**: [spec.md](../spec.md)
**Depth**: standard · **Audience**: author, before planning

## Requirement Completeness

- [x] CHK001 - Are the options to compare enumerated, with a minimum count? [Completeness, Spec §FR-001, US1]
- [x] CHK002 - Are the comparison dimensions for every option listed explicitly (cost, account/store, platforms, setup, live updates, push, background, terminal, exposure)? [Completeness, Spec §FR-001]
- [x] CHK003 - Is the behaviour when the Mac is asleep, off or unreachable a required comparison dimension, not only an edge case? [Gap → fixed, Spec §FR-001]
- [x] CHK004 - Are iPhone-specific limits (home-screen push, lock/background, sideloading without a paid account) required to be covered? [Completeness, Spec §FR-010]
- [x] CHK005 - Is the roadmap required to be ordered, sized and independently shippable? [Completeness, Spec §FR-005, US2]
- [x] CHK006 - Are the docs to update named, with what each must say afterwards? [Completeness, Spec §FR-008, US2 §3]

## Security Model

- [x] CHK007 - Are reachability requirements stated, including the public-internet exception rule? [Clarity, Spec §FR-003]
- [x] CHK008 - Are pairing, revocation and session lifetime each required to be defined? [Completeness, Spec §FR-003]
- [x] CHK009 - Is the stolen/lost-phone case required to be answered (what it can do until revoked, how to cut it off)? [Coverage, Spec §US2 §2, Edge Cases]
- [x] CHK010 - Is it required to state how today's origin/host protections are kept or replaced? [Consistency, Spec §FR-003]
- [x] CHK011 - Does the spec require extra protection for launch actions, given that launching an agent from the phone runs code on the Mac? [Gap → fixed, Spec §FR-003, §FR-012]
- [x] CHK012 - Is the "agent content shown on the phone must not become executable" edge case backed by a requirement? [Gap → fixed, Spec §FR-003]
- [x] CHK013 - Are requirements defined for what notifications may show on a locked phone (work content on the lock screen)? [Gap → fixed, Spec §FR-003]
- [x] CHK014 - Is "mobile off = today's exposure" stated measurably? [Measurability, Spec §SC-006, §FR-005]

## Work Mac

- [x] CHK015 - Is the fallback required when IT forbids a VPN/tunnel? [Coverage, Spec §FR-011]
- [x] CHK016 - Does the spec require stating what data leaves the work Mac and through which third parties (coordination servers, relays, push services)? [Gap → fixed, Spec §FR-011]

## Scope

- [x] CHK017 - Is the mobile scope enumerated (see, answer, launch) with the desktop-only rule for destructive actions? [Clarity, Spec §FR-012, §FR-004]
- [x] CHK018 - Is "destructive" defined with examples so the boundary is testable? [Clarity, Spec §FR-012]
- [x] CHK019 - Is the conversation-vs-terminal distinction on the phone required to be judged separately? [Coverage, Spec §US5]

## Acceptance Criteria & Consistency

- [x] CHK020 - Do US1 and FR-001 list the same set of options? [Consistency, Spec §US1, §FR-001]
- [x] CHK021 - Is the no-code-change boundary explicit, including where any experiment lives? [Clarity, Spec §FR-009]
- [ ] CHK022 - Can SC-004 (answer in under 30 s) be verified during the analysis, or only later? [Measurability, Spec §SC-004] — accepted: estimated now, verified when built (stated in the criterion).

## Notes

- CHK003, CHK011, CHK012, CHK013, CHK016 were gaps; the spec was updated in the same step.
- CHK022 left open on purpose: an analysis can only estimate it.
- After the experiment (2026-10-26): CHK007–CHK016 still hold. CHK010/CHK014 are now backed by
  evidence (Q2: today's `Host` guard refuses the tailnet name; accepting it without tokens gave
  a no-`Origin` caller the whole API). CHK022 stays open: US3 took ~63 s without push, not
  comparable to SC-004.
