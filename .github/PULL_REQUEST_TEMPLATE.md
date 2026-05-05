<!--
Thanks for contributing to Tuxlink!

Before opening this PR:
- [ ] Commits follow Conventional Commits (see CONTRIBUTING.md)
- [ ] Each commit ends with the `Agent: <moniker>` trailer
- [ ] Branch is `task-NN-<slug>` or `bd-<id>/<slug>` (per-task-branch model)
- [ ] Tests pass locally (cargo test + vitest)
- [ ] If this changes architecture, an ADR is added to docs/adr/
- [ ] If this is a UI change, manual browser smoke walked the user flow
-->

## Summary

<!-- 1–3 sentences. What does this PR do, and why now? -->

## Type

<!-- Pick one. Conventional Commit type the squash-merge will use. -->

- [ ] `feat`: new user-visible feature (MINOR bump)
- [ ] `fix`: bug fix (PATCH bump)
- [ ] `perf`: performance improvement (PATCH bump)
- [ ] `refactor`: internal restructuring (PATCH bump)
- [ ] `docs`: docs only (no bump)
- [ ] `test`: tests only (no bump)
- [ ] `build` / `ci` / `chore`: tooling (no bump)
- [ ] **BREAKING CHANGE** (MAJOR / pre-1.0 MINOR — describe the contract surface affected)

## Scope

<!-- Pick from the table in CONTRIBUTING.md: protocol / pat / wizard / mailbox / compose / session / menu / tray / shell / config / appimage / ci / docs / pitfalls / adr -->

## Test plan

<!-- Bulleted checklist of how this was verified. -->

- [ ] `cargo test` passes
- [ ] `pnpm vitest run` passes
- [ ] `cargo clippy -- -D warnings` clean
- [ ] Browser smoke (if UI): _describe what flow was walked_
- [ ] Live-CMS smoke (if Pat-touching): _describe operator action, or N/A_

## Live amateur radio operations

- [ ] This PR does NOT introduce any code path that transmits on real amateur-radio infrastructure under automation, OR
- [ ] This PR introduces such a code path AND the live-CMS consent gate is wired up per [docs/live-cms-testing-policy.md](../docs/live-cms-testing-policy.md)

## ADR reference

<!-- If this PR enacts an architectural commitment, link to the ADR -->

- [ ] No new ADR (incremental change within existing architecture)
- [ ] New ADR added: `docs/adr/NNNN-<slug>.md`

## CHANGELOG note

<!-- release-please auto-generates from the squash-merge subject. If your subject doesn't capture the user-visible impact well, add a note here so the maintainer can tweak the merge message. -->
