# Engineering guide

## Parallel Pull Requests

Many pull requests are open at once, so code that every feature appends to is
a merge-conflict hotspot. Most past conflicts were two branches adding lines
at the same spot, or both bumping one hard-coded count.

- Do not assert hard-coded totals such as `assert_eq!(list.len(), 55)` or
  fixed array lengths like `[T; 98]` for lists features grow. Compare against
  the source list, assert uniqueness or membership, or use a slice.
- Declare each item of a growing list once. Prefer one table, or a
  `macro_rules!` table, that generates the matching `match` arms and `ALL`
  lists over parallel lists edited in several places.
- Keep lists whose order carries no meaning (enum variants without derived
  ordering, asset tables, registries) sorted between `// keep-sorted start`
  and `// keep-sorted end` markers. Additions then land on different lines.
  `just check-sorted` (part of `just ci`) enforces the markers and `just sort`
  fixes them. Never mark a list whose order is shown to users or is a wire
  contract, such as menu rows or the vendored bincode types.
- Give a new feature's failures their own `thiserror` enum named `Error` in
  its module's `error.rs`, with variants not prefixed by the feature name, and
  add one `#[error(transparent)] Feature(#[from] feature::Error)` variant to
  the crate's root `Error`, as `pane_move` and `backdrop` do, rather than a
  run of variants in `error.rs`. The root enum stays sorted. Callers name it
  `feature::Error`, never through an `as FeatureError` alias.
- Wrap another crate's error the same way (`GitHub(#[from] sheprd_github::Error)`).
  Do not mirror its variants in the app's root and convert one by one: each
  failure has one definition, and callers match the nested variant.
- Shared state structs (`SheprdWindow`, `Config`, `herdr_settings::Settings`)
  and their constructors keep their fields sorted. Run an initializer that has
  an effect or depends on another (registering a subscription, creating an
  entity, reading a value before it moves) in a `let` before the struct
  literal, so the literal itself has no order to preserve. Derive `Debug`
  rather than listing fields by hand; redact a private field through its own
  type's `Debug`.
- `just hooks` also enables `rerere`, so a resolution is reused when the same
  conflict returns on another branch, and a `Cargo.lock` merge driver that
  keeps one side and has Cargo rebuild it before the commit. After a clean
  merge whose only conflict was the lockfile, the merge stops with the rebuilt
  lockfile staged: run `git commit --no-edit` to finish it. The rebuild
  refuses while a `Cargo.toml` or `Cargo.lock` change is unstaged, and an
  aborted merge's pending rebuild is dropped with a warning. Squash merges
  rebuild like merges. Cherry-picks, reverts, and rebases commit without
  these hooks, so they stop with `Cargo.lock` conflicted: run
  `cargo metadata`, `git add Cargo.lock`, and continue. GitHub's Update
  branch button runs no hooks; there, run `cargo metadata` and commit
  `Cargo.lock` by hand.

## Command Line

`sheprd COMMAND` drives the running app over its control socket, so
agents in Herdr panes can use what only this client shows or keeps
(`docs/cli.md`). Agents learn about it from the managed skill in
`skills/herdr-gpui-browser/SKILL.md`, which the app installs and refreshes.

- When a feature adds something an agent or script could reasonably ask for
  (open or focus a view, read state the app holds, change a setting, wait for
  an event), add a command in the same change: the request in
  `sheprd-control-protocol`'s `protocol`, its handler in `control/server`,
  parsing and output in `control/client`, its `--help` text, and tests for
  each.
- Do not add a command that repeats Herdr's own `herdr` CLI (workspaces,
  terminal tabs, panes, agents, scrollback), that the app cannot really do,
  or that skips a confirmation. Destructive or outward actions open the
  app's own dialog, pointed at the caller's checkout or the machines named,
  and the user confirms there; a batch of them is confirmed once. The one
  exception is `cloud exec`, a command on a cloud machine the user saved,
  which is the agent's own work and asks nothing, as the agent's shell does
  not.
- Keep the shared contract: `--json` and `--window` on every command, the
  exit-code table in `docs/cli.md`, bounded requests and responses, and
  targeting that refuses rather than acting in another session's window.
- Add the command to `docs/cli.md`. When an agent should reach for it
  unprompted, also add it to the skill's command table and, if it is a new
  kind of task, to the skill's `description`, which is all an agent sees
  before loading it. Keep the skill's `name`: installed copies are found
  by it.

## Rust Conventions

- Require only the capabilities an operation uses. Prefer existing traits such as `Read`, `Write`, `IntoIterator`, `AsRef<Path>`, and `AsRef<OsStr>` over specific streams, collections, or owned paths.
- Use slices, `&str`, and `&Path` when borrowing is sufficient. Keep owned data where it must cross a thread boundary or outlive its caller.
- Prefer `impl Trait` or named generics for static dispatch. Use `dyn Trait` for actual runtime heterogeneity/type erasure, not by default.
- Use `FnOnce`, `FnMut`, or `Fn` for injectable behavior. Do not invent a callback or clock trait when a closure or explicit time value suffices.
- Do not introduce custom traits merely to wrap one struct. A new trait needs a concrete behavioral contract and a real implementation/substitution need that existing traits cannot express.
- Use enums for closed alternatives such as navigation targets, pane/popup input destinations, connection phases, and launch modes. Avoid string dispatch and boolean-plus-ID pairs internally.
- Parse strings at boundaries, then match on domain types. Prefer `From`, `TryFrom`, `Into`, and `TryInto` for conversions.
- Prefer typed request/result structures when shapes are known. Preserve genuinely open-ended protocol envelopes rather than forcing a speculative schema.
- Derive standard traits such as `Default`, `PartialEq`, and `Eq` where their semantics are valid. Compare complete values when deduplicating operations.
- Every crate must expose its own root `Error` and `Result<T, E = Error>` (`pub use error::{Error, Result}`), with errors defined using `thiserror`. Use meaningful typed variants for internal failures and match on variants, never error text.
- Use `anyhow::Result` and context at application/reporting and test-harness boundaries, not as a replacement for typed internal errors. Keep dependency versions in workspace dependencies.
- Never use `String` or `&str` as an error type, including `Result<(), String>`, worker-channel results, and intermediate operation state. Strings are allowed as diagnostic payloads, unchanged wire-protocol fields, and final UI display text, not as catch-all internal error wrappers.
- Propagate errors with `?`; preserve original causes with `#[from]`/`#[source]` and retain structured operation/path context until the display boundary. Do not flatten sources with `to_string()` or `format!()` during propagation. Keep `io::Result` where an actual I/O trait or connector contract requires it, preserving typed sources when adapting errors.
- Test error variants and source chains as well as user-facing diagnostics. Preserve redaction, bounded remote diagnostics, retry/cancellation classification, and protocol compatibility when changing error handling.
- No `unwrap()` or `expect()` in production. Test-only allowances must be scoped to test code.
- Prefer safe Rust. Do not introduce `unsafe` unless there is no practical safe alternative; investigate standard-library APIs and maintained safe wrappers first. Any unavoidable exception must be tightly scoped and document both why safe alternatives are insufficient and the safety invariants.
- Prefer guard clauses and readable iterators. Avoid clones, allocations, helper layers, and generic parameters that provide no benefit.
- Comments should explain invariants, ownership, or non-obvious decisions, not narrate assignments.

## GPUI Rules

- Treat app, window, and entity contexts as UI-thread execution. A foreground `spawn` future does not make blocking work safe.
- Never perform blocking socket/disk I/O, process waits, sleeps, or expensive background computation on the UI thread. Use the existing worker threads or GPUI background executor, then apply results through the appropriate context update.
- Start child processes with `sheprd_client::new_command`, never `Command::new` (clippy rejects it outside tests). On Windows a console child built any other way can open a window of its own.
- Keep render and input paths thin. Render from prepared state and bounded caches; do not query the daemon, scan files, or launch processes while rendering.
- Preserve continuously drained ordered events and the coalesced latest-state mailbox. Do not replace them with an unbounded UI event queue.
- Route connection/startup failures through the authoritative inbox. Reconnect and detach must isolate old inboxes so delayed events and paint acknowledgements cannot affect the new connection.
- Derive connection text/indicators from `ConnectionStatus`. Old local operation errors must not hide a disconnection reason.
- Queue success is not daemon acknowledgement. Keep names such as `last_queued_options` honest and include cell pixel metrics in resize comparisons.
- Use the same geometry for painting, hit testing, and IME placement. Popup input and composition bounds must target the popup, not the underlying pane.
- Keep glyph visibility separate from decorations: spaces and wide-character continuation cells may still need underline/strikethrough painting.
- Preserve menu input isolation, focus behavior, Unicode composition, and semantic input routing. Do not synthesize terminal keys for operations with endpoint API methods.
- Activity indicators come from the daemon's snapshot, not guessed terminal output or a client-local seen ledger. Every client shows the same dot; do not reintroduce per-client acknowledgement without asking. Unread agents are the one read state, and it is not per client: each host's `sheprd-remote-control` keeps it (`control::read`), so every desktop and phone shows the same unread dots.
- Animate UI that appears, disappears, moves, or changes in response to a click or hover when motion helps the user follow it: tabs, panels, popovers, revealed controls. Animate both ways, so what slides in slides back out, and reverse from where it is when interrupted. Skip it where motion would only delay the result, such as typing, scrolling content, or terminal output.
- Time animations through `crate::motion`: read `motion::now()` instead of `Instant::now()` for start stamps and progress, pass GPUI `Animation` lengths through `motion::duration`, and prefer `motion::ENTER`/`LEAVE`, `progress`, and `Toggle` over new curves and constants. That keeps motion consistent and lets QA > Slow Animations slow every animation tenfold.

## Protocol And Safety

- Vendored bincode field and enum variant order is a compatibility contract. Do not reorder declarations as cleanup. Preserve attribution in `crates/sheprd-protocol/NOTICE.md`.
- Keep frame/response/geometry bounds, strict decoding, and atomic patch validation. Test malformed inputs and limits, not only valid fixtures.
- Never mix snapshot metadata and surface cells across boot IDs or projection revisions. Preserve future-surface buffering and stale-boot rejection.
- Preserve the single in-flight API request, bounded command/event queues, FIFO ordering, bounded write batches, and request correlation.
- Partial frame prefixes/payloads must survive read timeouts. Finish an inbound partial frame before dispatching commands against potentially stale state.
- Cancellation must not flush queued work, replay commands, or join a worker on the UI thread. Do not add automatic reconnect/replay as an incidental refactor.
- Crossbeam receiver clones compete for events; they are not broadcast subscribers.
- Treat terminal content and daemon messages as untrusted data. Do not execute terminal escapes, follow graphics file paths, or run displayed update commands automatically. The one exception is the daemon's OSC 52 `ServerMessage::Clipboard`: decode it and put only its bounded UTF-8 text on the pasteboard, never arbitrary bytes, files, or images.
- The normal app never installs, stops, migrates, or upgrades a personal daemon. It starts one only to connect: `herdr server` when Herdr 1 is installed, and herdr-core for Herdr 2 terminals (or when no Herdr 1 is installed), the user's own or else the pinned one every release bundles beside the app as a companion like `sheprd-acp` (`scripts/release/herdr-core.json`, licenses in `herdr-core-licenses.txt`), never when a core already answers. Herdr 1 sessions stay where they are: moving users to Herdr 2 is their choice, never an automatic switch. Explicit sockets refer to the binary client socket, not the JSON API socket.
- Never commit secrets, credentials, private terminal output, or machine-local configuration.

## Dependencies And Features

- Put dependency versions in root `[workspace.dependencies]`; member crates inherit with `{ workspace = true }`.
- Use the pinned `rust-toolchain.toml` and GPUI version. Do not copy toolchain, feature, or release assumptions from Moltis/Arbor.
- Keep workspace lint inheritance and fix warnings rather than widening allowances.
- Keep `integration-test` opt-in. Gate imports, modules, and call sites consistently, and verify both default and all-feature builds.
- Do not introduce Tokio or another runtime to wrap the existing blocking socket worker.
- Avoid unrelated lockfile upgrades. Use `--locked` for reproducible verification.

## Changelog

Users learn what changed from the GitHub release body, which is generated from
git history by `git-cliff` (`cliff.toml`) rather than written by hand. The
release workflow's `changelog` job runs `scripts/release/generate-changelog.sh`
and `publish` passes its `RELEASE_NOTES.md` to `gh release create --notes-file`.
`CHANGELOG.md` is generated for each release and is not tracked in git.

- Commit subjects are the changelog entries, verbatim apart from a capitalized
  first letter. Write the subject line for a user reading a release page, and
  put rationale in the body; only the first line is published.
- The type chooses the section: `feat` becomes Added, `fix` becomes Fixed,
  `perf`/`refactor`/`revert` become Changed, `docs` becomes Documentation, and a
  `(security)` scope becomes Security. A scope prints as a `[scope]` prefix, and
  anything non-conventional falls into Changed.
- `chore`, `ci`, `build`, `style`, `test`, and merge commits never reach users,
  so nothing user-visible may ship under those types. A breaking change is the
  exception: `type!:` or a `BREAKING CHANGE:` footer is always published, under
  Changed and prefixed `**BREAKING:**`, whatever its type.
- Previewing the generated changelog before a release is covered by the
  `release` skill.
- These rules are enforced, not advisory. `just hooks` installs a `commit-msg`
  hook that refuses a bad subject before the commit exists, and CI's `commits`
  job checks every commit in a pull request; it is part of the `Format, lint,
  and test` gate. Run `just hooks` once per clone, and `just check-commits`
  to check a branch by hand. Fix a rejected subject with `git commit --amend`
  rather than `--no-verify`.

## Verification

Prefer the existing `just` recipes. Before handoff, a requested commit, or a
push, run relevant quick checks expected to finish in under 10 seconds each,
including compilation time. Keep the overall local validation pass short;
defer longer tests, full workspace gates (`just ci`), cross-platform lints,
and release builds to CI jobs. Run longer checks locally only when explicitly
requested or needed to investigate a specific failure. Report what ran and
what was deferred to CI.

- For linking, startup, or packaging changes, rely on CI build and CLI smoke checks; use `just test-build` locally when requested or needed to investigate a failure.
- The `windows` CI job checks formatting, lints every target and feature, and
  runs workspace tests with default and all features, including headless UI and
  CLI tests, split like `checks` into parallel `default` and `all` legs. The
  `windows-native` job runs `live_gui` (the `test-gui` and `test-sidebar`
  equivalents) against a pinned `herdr.exe` on both Windows runners, with
  every platform, when what the app is built from changes
  (`scripts/ci-changes.sh`), except the live-daemon test on ARM64, where the
  emulated daemon ignores Enter; its release-mode performance budget runs
  on main and in the merge queue only. The optimized Windows build and its
  installer run in `windows-package` when their inputs change, with every
  platform, and in
  the release workflow, which only smoke-tests the CLI of the bytes it ships
  and requires CI to have passed on the commit. Keep the `cfg(unix)`/`cfg(windows)`
  split honest and say plainly what a platform cannot do instead of stubbing
  behavior that appears to work.
- Pull requests run the Linux checks and the macOS default-features leg,
  and lint and test only the crates their changes reach
  (`scripts/ci-scope.py`): on ARM64 for members' pull requests, whose runs
  take our own machines, and on x86_64 otherwise. `main`, the merge queue,
  and a pull request that changes a platform `cfg` line or file, a
  dependency (any `Cargo.toml`, `Cargo.lock`) or the toolchain, or the CI
  selection itself, or carries the `windows`, `macos`, or `full-ci` label,
  run every platform over the whole workspace, except that such a pull
  request runs Windows only for what a Windows build reads: not for macOS
  files, `cfg` gates naming only macOS or Apple, or the `macos` label. The
  Nix flake builds on pull requests only when the flake or the toolchain pin
  changes, and on `main` and every release otherwise.
- macOS jobs, and the ARM64 Linux legs, run on our own machines when the
  repository variables `CI_MACOS_RUNNER` and `CI_LINUX_ARM_RUNNER` name their
  labels (`docs/self-hosted-runners.md`), for
  members' code only: `main`, the merge queue, and pull requests from
  branches of this repository opened by its owner, members, or
  collaborators. A fork's or a bot's pull request always uses hosted runners.
  A runner that outlives its job keeps `target/` between runs
  (`scripts/ci-clean-self-hosted.sh`).
- A `cfg` gate is only proven on the platform it names. For changes that touch
  one, rely on the corresponding platform CI jobs: a macOS run cannot see a
  Linux-only gate, and neither can the Windows cross-check. Code inside an
  existing gate changes no `cfg` line, so label such a pull request
  `windows` or `macos`. `just lint-linux` and `just lint-windows` are
  available for local investigation when needed.
- Documentation-only changes need command/path/link review and `git diff --check`; do not claim a code test run that did not happen.
- Add deterministic tests for invariants: ordering, cancellation, fragmentation, revision fencing, error paths, geometry, and cleanup. Prefer explicit coordination and bounded waits over timing guesses.
- Test pure session/CLI/geometry logic without a socket or window where possible. Exercise public behavior with mock peers and headless GPUI tests where integration matters.
- Keep test bodies out of production files, which only declare `#[cfg(test)] mod tests;`.
  Put tests for a new feature in a new topic file under the module's `tests/` directory
  (for example `keymap/tests/pane_keys.rs`, starting with `use super::*;`) rather than
  inserting them into an existing large test module, and keep shared fixtures in the
  parent `tests.rs`. Parallel PRs then edit different files instead of conflicting.
- Fix flaky tests rather than hiding them with retries or new ignores. Existing live/native tests are ignored because they require explicit external resources.
- For visual changes, verify the actual native UI when a desktop is available, including narrow layouts, long labels, focus, popup/IME behavior, and clipping. Headless layout tests do not prove native glyph or OS input correctness.
- Measure interactive performance in release mode (`just run`), not debug mode. Preserve bounded glyph caching and deterministic paint budgets; native timing budgets are machine-dependent.
- Keep CLI parsing pure and based on OS strings so socket paths need not be UTF-8. Invalid options and help must exit before starting GPUI.

### Opt-In Tests

```sh
just test-live /absolute/path/to/herdr
just test-core /absolute/path/to/herdr-core
just test-gui /absolute/path/to/herdr
just test-sidebar
just test-perf
```

- Live tests require an explicitly selected `HERDR_TEST_BINARY`; never fall back to discovering a user's daemon.
- Reuse the shared sandbox: private unique short directory, cleared environment, isolated HOME/XDG/config/socket paths, and explicit desktop-variable allowlist for GUI children only.
- Kill and wait only for exact children created by the test, before removing their sandbox. Never use `server stop`, process-name matching, or process-group kills. The one exception is herdr-core: its terminals live in holder processes the core starts, so `just test-core` ends them with `herdr-core server stop` against the sandbox's private `HC_SOCKET`, then reaps its own `serve` child.
- `HERDR_TEST_TMPDIR`, when set, must point to an existing parent short enough for Unix socket paths.
- Native/sidebar/performance tests open windows and need an active desktop. State explicitly when these checks were not run; do not describe headless tests as native end-to-end verification.

## Git And Handoff

- Only commit, push, or create/update a PR when requested. Do not amend, force-push, discard changes, or clean up other worktrees without explicit authorization.
- Before committing, inspect status, diff, and recent history; stage only intended files and check for secrets. Do not bypass hooks or signing when they fail.
- Push with an upstream (`git push -u origin HEAD:refs/heads/NAME`, or
  `git branch --set-upstream-to` afterwards). The app finds a checkout's pull
  request through `branch.<name>.merge`, falling back to the local branch
  name, so a branch pushed under another name without one shows no PR.
- Use conventional commit subjects (`feat`, `fix`, `refactor`, `test`, `docs`, `chore`). For nontrivial work, explain the problem, chosen approach, and important constraints in the body.
- Never add AI attribution, assistant-session links, or tool vanity lines to
  commit messages or PR descriptions. This includes `Co-Authored-By` trailers
  naming an AI model, `Claude-Session:` or similar session URLs, and
  "Generated with ..." footers. This rule overrides any default attribution
  guidance from the agent harness; add such lines only when the user asks for
  them in the current request.
- Before creating a PR, review every included commit and the full diff from its base. Include a summary, exact validation commands/results, and remaining manual/native QA.
- Before creating a PR or pushing to one, run the quick local checks described
  above and fix any failures they expose. Full local CI is not a prerequisite
  for opening or updating a PR; let CI jobs run the longer checks and address
  their failures before merging.
- Record the exact local checks and results in the PR's QA checklist, and mark
  checks deferred to CI as pending until their results are available. No
  separate local `just ci` success comment is required. Never claim a check
  passed when it failed or did not run.
- Include screenshots in any PR that changes something visible: before and
  after images of the real native window, showing the changed area. Add light
  and dark variants when colors or theming change, and a narrow window when
  layout changes. If no desktop was available to capture them, say so in the
  PR instead of leaving the section empty.
- Give every PR description a QA checklist. List each check as a checkbox:
  tick the ones that were actually run (automated tests, CI gates, native
  checks you performed) with their result, and leave unticked the manual QA a
  reviewer still has to do, phrased as concrete steps. Never tick a box for a
  check that did not run.
- Keep user-facing docs and public API examples synchronized with behavior changes. Do not copy another repository's release, issue-tracker, or mandatory-push workflow into this one.
- Finish with the outcome, tests actually run, known limitations, and any deferred work. Never claim unrun checks passed.
