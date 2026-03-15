# Release Workflow

This project uses Git tags as the release source of truth.

## Versioning
- Tag format: `vMAJOR.MINOR.PATCH` (example: `v0.1.0`).
- Build metadata is auto-generated from Git history:
  - `<tag>+build.<commit-count>.<short-sha>`
  - `.dirty` is appended for uncommitted worktrees.

## Local Release Steps
1. Ensure clean working tree:
   - `git status --short`
2. Run validation:
   - `cd firmware && pio run`
   - `cd host/studio && cargo test`
3. Create release commit if needed:
   - `git add . && git commit -m "chore: prepare release vX.Y.Z"`
4. Create tag:
   - `git tag vX.Y.Z`
5. Push branch and tags:
   - `git push origin main`
   - `git push origin --tags`

## GitHub Behavior
- Pushing a `v*` tag triggers `.github/workflows/release.yml`.
- The workflow builds firmware and host validation.
- Firmware binary is uploaded as workflow artifact.
- A GitHub Release is created for the pushed tag.

## Notes
- Keep tags immutable. If a tag is wrong, create a newer corrective tag.
- Prefer patch bump (`vX.Y.Z+1`) for non-breaking fixes.
