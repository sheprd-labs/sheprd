# AGENTS.md

Engineering guidance for agents working in this repository. Read this alongside
`README.md`, the relevant crate documentation, and the code before making changes.

## Priorities

- Keep changes small, explicit, and focused on the root cause.
- Preserve user and other agents' changes. Never revert unrelated edits.
- Prefer existing standard traits and generics over unnecessarily concrete APIs.
- Protect protocol compatibility, bounded resource use, and UI responsiveness.
- Add regression tests for changed behavior and report what was actually verified.
- Never modify or patch Herdr itself. It is owned by an unrelated external team;
  sibling Herdr checkouts are read-only references. Propose an upstream PR only
  when genuinely justified, never make incidental upstream edits for this client.

## Architecture

Sheprd is a native client of Herdr 1 and herdr-core, not a TUI wrapper.
Herdr 1 supplies terminal cells; herdr-core supplies raw output emulated by
`sheprd-terminal`. The daemons own terminal processes and session state.
Closing or detaching the GUI must leave the daemons and their terminals running.

Consult the [crate map](docs/crate-map.md) when choosing where a change belongs.
Keep crate descriptions there so this file stays within the 32,768-byte project instruction budget.

- Keep dependency direction from UI to client to protocol. Protocol/client code must not depend on GPUI.
- Keep session transitions separate from socket scheduling, and connection ownership separate from window rendering.
- Reuse `ConnectionBridge`, domain targets, geometry helpers, and the test sandbox rather than duplicating their policies.
- **File size limit: 1,000 lines.** No Rust, Python, shell, or Swift file may exceed it,
  tests included; `just check-file-size` enforces it in `just ci` and CI's checks job.
  There is no allowlist: when a change would cross the limit, split the file by
  responsibility in the same change rather than adding an exception.
- Split modules by responsibility; the line limit is a ceiling, not a reason for arbitrary cuts. Prefer wiring and exports in entry points as code grows; do not perform unrelated file reshuffles.
- Keep APIs narrow. Use private items by default and `pub(super)` or `pub(crate)` only where needed; do not expose every field merely to ease extraction.
- Define shared types once in the lowest appropriate layer and re-export them. Do not create mirror enums or convert between them through strings.
- Do not add a crate, runtime, service container, or speculative transport abstraction without a concrete need. Sibling checkouts are references, not build dependencies.

## Implementation

Read the linked sections for the change you are making.

- For Rust changes, follow the [Rust conventions](docs/engineering-guide.md#rust-conventions) and [dependency rules](docs/engineering-guide.md#dependencies-and-features). Preserve typed errors and their sources. Avoid production `unwrap()` and `expect()`.
- When editing registries, shared structs, error enums, or resolving lockfile conflicts, follow the [parallel PR guidance](docs/engineering-guide.md#parallel-pull-requests).
- For native UI work, follow the [GPUI rules](docs/engineering-guide.md#gpui-rules) on threading, rendering, input, activity, and motion. Keep blocking work off the UI thread.
- For daemon, socket, or terminal changes, follow the [protocol and safety rules](docs/engineering-guide.md#protocol-and-safety). Treat terminal output as untrusted and preserve users' running sessions.
- When a feature exposes app state or actions that scripts could use, add its command, tests, help, and docs in the same change. Follow the [CLI contract](docs/engineering-guide.md#command-line).

## Verification

Run relevant quick checks expected to take under 10 seconds each, including compilation. Leave longer checks to CI unless requested or needed to investigate a failure. For docs, review commands, paths, and links and run `git diff --check`.

Follow the [verification guidance](docs/engineering-guide.md#verification) for test placement, native UI checks, platform coverage, and PR labels. Before running [opt-in tests](docs/engineering-guide.md#opt-in-tests), check their sandbox and cleanup requirements. Live tests must use an explicitly selected daemon binary.

## Changelog

Commit subjects become release notes. Use a conventional subject that describes the user-visible change and follow the [changelog rules](docs/engineering-guide.md#changelog).

## Git And Handoff

Commit, push, or create or update a PR only when requested. Get explicit authorization before amending, force-pushing, discarding changes, or cleaning up other worktrees. Never commit credentials or private terminal output.

Follow the [Git and handoff checklist](docs/engineering-guide.md#git-and-handoff) for commits and PRs, including native screenshots for visible changes. At handoff, report what changed, which checks ran, and what remains unverified.
