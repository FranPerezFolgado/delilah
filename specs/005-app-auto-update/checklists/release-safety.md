# Release & Update Safety Checklist: App auto-update

**Purpose**: Unit tests for the requirements on update integrity, data safety across versions, the release process and the update UX states
**Created**: 2026-10-23
**Feature**: [spec.md](../spec.md)
**Audience / depth**: reviewer before the PR; standard

## Update integrity

- [x] CHK001 - Is it specified that an update is installed only after its signature is verified, and refused otherwise? [Completeness, Spec §FR-010]
- [x] CHK002 - Is "newer" defined strictly, so equal or older manifest versions are never offered (no downgrades)? [Clarity, Spec §FR-003, Edge Cases]
- [x] CHK003 - Is it stated where the private signing key lives and that only the public key ships with the app? [Completeness, Spec §FR-019]
- [x] CHK004 - Is the update location's transport and authority defined (one fixed public location, no user-supplied URL)? [Gap → resolved: Spec §FR-016, Clarifications]
- [x] CHK005 - Are requirements defined for losing or rotating the signing key (apps that ship the old public key)? [Gap → resolved: Spec §FR-021, Edge Cases]

## Data safety across versions

- [x] CHK006 - Is it specified that all user data survives an update, with the kinds of data listed? [Completeness, Spec §FR-013]
- [x] CHK007 - Is the pre-upgrade database copy defined: when it is taken, how it is named and how many are kept? [Clarity, Spec §FR-014, Clarifications]
- [x] CHK008 - Is the failure path defined when the new version cannot bring the database up to date? [Recovery, Spec Edge Cases]
- [x] CHK009 - Is it explicit that there is no automatic rollback, and is that recorded as an assumption? [Assumption, Spec §Assumptions]
- [x] CHK010 - Is "running session" defined precisely enough to count (working vs. waiting on the user)? [Clarity, Spec §FR-009]
- [x] CHK011 - Is it specified what becomes of sessions stopped by an update after the relaunch? [Recovery, Spec §US3-6]
- [x] CHK012 - Is the behaviour defined when the app quits while an update waits for sessions? [Edge Case, Spec §US3-4]

## Release process

- [x] CHK013 - Is the list of places where the version must match referenced, and is a mismatch a hard stop? [Consistency, Spec §FR-017]
- [x] CHK014 - Is the set of published files enumerated (disk image, package, signature, manifest, notes)? [Completeness, Spec §FR-016]
- [x] CHK015 - Is the source and format of release notes defined? [Clarity, Spec §FR-018]
- [x] CHK016 - Is the behaviour defined when publishing fails half-way (some files uploaded, manifest not)? [Gap → resolved: Spec §FR-022]
- [x] CHK017 - Is the order of publishing specified so installed apps never see a manifest that points at files not yet uploaded? [Gap → resolved: Spec §FR-022]
- [x] CHK018 - Is the bootstrap path from versions without the updater documented as a requirement? [Dependency, Spec §FR-021, Assumptions]
- [x] CHK019 - Is the dependency on the repository being public stated and checked before release? [Assumption, Spec §Assumptions, §FR-017]

## Update UX states

- [x] CHK020 - Are all user-visible states listed: checking, up to date, available, downloading, waiting for sessions, installing/relaunching, error? [Completeness, Spec §FR-023]
- [x] CHK021 - Is each error state's message and next action (retry, keep current) specified? [Coverage, Spec Edge Cases, §FR-011]
- [x] CHK022 - Is "Más tarde" scoped (per version, until next launch or manual check) consistently between US1-4 and FR-008? [Consistency]
- [x] CHK023 - Is the periodic check interval quantified and consistent between FR-001 and SC-002? [Consistency, Spec §FR-001, §SC-002]
- [x] CHK024 - Is it specified whether automatic checks are silent on failure and manual ones are not? [Clarity, Spec §FR-004]
- [x] CHK025 - Is the behaviour defined when a newer release appears while an older one is downloading or waiting? [Gap → resolved: Spec Edge Cases]
- [x] CHK026 - Are UI texts given in the app's languages (Spanish/English, as the rest of the UI)? [Gap → resolved: Spec §FR-023]
- [x] CHK027 - Are dev builds and previews excluded explicitly? [Coverage, Spec §FR-005]

## Measurability

- [x] CHK028 - Can SC-001 (one decision, under 2 minutes) be measured without implementation knowledge? [Measurability, Spec §SC-001]
- [x] CHK029 - Is SC-004 (tampered packages never installed) backed by a defined set of tamper cases? [Measurability, Spec §SC-004 → wrong signature, missing signature, package altered after signing]

## Notes

- Gaps found in the first pass (CHK004, CHK005, CHK016, CHK017, CHK020, CHK025, CHK026, CHK029) were fixed in spec.md on 2026-10-23 before planning.
