# Scratch descriptor voice retirement — 2026-09-28

The user explicitly retired the old **Descriptor voice**. Scratch now has one
voice: explicit four-stroke cylinder events. The selector and descriptor-only
tone controls are removed. The event synth is required by `ScratchModel`, not
an optional fallback. New engines and rebuilt engines always use it.

Older projects with `scratch.engine: "experimental"` deserialize to
`standalone`; saving writes the current name. Their design, builder settings,
event calibration and listening settings are retained. Their sound changes to
the remaining voice. Unknown engine names still fail validation.

The generated grid still supplies internal texture and level envelopes used
by the event engine. Imported-bank synthesis remains available. This removal
does not implement the future physical engine simulation (W-006).

Tests exposed a pre-existing event-path problem: replacing the generated
pressure also discarded afterfire bursts. Afterfire is now a separate transient
added after pressure replacement and still passes through the exhaust network
and listening limiter. The existing lift-off/optional-pops checks now pass for
the remaining voice.

The old descriptor-brightness test now tests event pulse width. The
descriptor-only half-order idle test was retired with that voice; its results
are not claimed for the event engine. Crank-coupled idle realism remains open
under W-005.13 / W-006.

Validation: the full release suite passes 126 tests, including 18 scratch tests
covering legacy project migration and
round-trip, identical migrated/explicit-event renders, live rebuilds, arbitrary
firing orders, output bounds, level, fuel cut and afterfire. Strict Clippy passes
for all targets; release build and `git diff --check` pass. Logs are in
`output/event-only-*20260928.txt`.

The normal executable is rebuilt at `target/release/bess.exe`. The running
previous build was preserved as `target/release/bess-before-events-only-20260928.exe`;
restart BESS to use the new build. Subjective listening remains unvalidated.

Executable SHA256: `4EEBB0FBFB4CD35CF0C9D7E1E0ECCEC86183AF000D7222333524EB7439FC1B2E`.
