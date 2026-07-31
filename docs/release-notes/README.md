# Release notes

One file per release, named for its tag: `v0.1.12.md`.

The `GitHub Release` stage in the `Jenkinsfile` publishes `docs/release-notes/<tag>.md`
as the release body when the file exists, and falls back to `gh`'s generated
commit list when it doesn't. Committing the notes with the version bump keeps
them reviewable in the same diff as the change they describe, and means a
release published by CI reads the same as one published by hand.

Write for someone using the app, not reading the log: name the symptom they
saw, then what changed. "If refines were failing on claude-sonnet-5, this is
why" beats "fix temperature handling".
