# FluxWM

A Rust Wayland compositor project with scrolling columns, a two-axis canvas, GPU rendering, animation and Lua configuration.

**Status:** early implementation, not production-ready. The workspace contains a compiling Smithay nested-compositor bootstrap and independently developed core, layout, Lua helper and IPC components. They are not yet an integrated desktop; no native-session, GPU hardware, secure-lock or complete phase qualification is claimed.

Start with the [current production-readiness audit](docs/24-implementation-readiness-audit.md), [documentation index](docs/README.md) and [ten-phase roadmap](docs/10-roadmap.md). The audit identifies concrete source findings, blockers and evidence limits. The [112-case catalog](docs/15-edge-cases.md) specifies future qualification, not runtime passes.

Run document checks with `python3 tools/check_docs.py`. Component build/test commands and remaining integration work appear in [development documentation](docs/development/README.md). Historical and current [independent Claude review records](docs/reviews/README.md) preserve what was actually reviewed.
