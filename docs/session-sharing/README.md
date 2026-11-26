# Shared-session design and evidence

The [current plan](../session-sharing-plan.md) is authoritative. The user approved
phased implementation on 2026-11-22 after reviewing Phase 0.

- [Implementation progress](implementation.md).
- [Implemented native gateway profile](gateway.md).
- [Interactive sharing, browser, and native mockups](mockups/index.html).
- [Original annotated plan review](plan-review/index.html), preserved as reviewed.
- [Phase 0 report page](phase0-review/index.html), preserved as reviewed.
- [Phase 0 experiment source archive](phase0-experiment.zip), with reproduction
  instructions, dependency lock, synthetic browser/native harness, and evidence.
- [Current technical report](../session-sharing-phase0.md) and
  [recorded probe results](../session-sharing-phase0-evidence.json).

The review pages are historical design artifacts. Their approval prompts and
implementation status predate the approval in chat; they do not override it.
Mockup controls have no connection to a real agent. The experiment is a loopback
fixture, not a deployable server. Its disposable tunnel copy is evidence, not a
second production implementation. Generated WASM, build products, downloaded
tools, credentials, and real session content are excluded.

Each HTML folder is self-contained and can be opened or shared on its own.
The three PNGs in `mockups/` provide static previews without a browser.
