# FluxWM

A Rust Wayland compositor project with scrolling columns, a two-axis canvas, GPU rendering, animation and Lua configuration.

**Status:** early implementation, not production-ready. The workspace now has a [runnable nested compositor](docs/development/runtime.md) with managed Wayland lifecycle, strip/canvas layout and Unix IPC. Lua schema evaluation remains a separate helper; animations and complete desktop integrations remain unfinished. Native-session, physical presentation, secure-lock and complete phase qualification are not claimed.

Start with the [current runtime findings and production gates](docs/development/runtime-findings.md), [historical initial production-readiness audit](docs/24-implementation-readiness-audit.md), [documentation index](docs/README.md) and [ten-phase roadmap](docs/10-roadmap.md). The audit identifies concrete source findings, blockers and evidence limits. The [112-case catalog](docs/15-edge-cases.md) specifies future qualification, not runtime passes.

Run document checks with `python3 tools/check_docs.py`. Component build/test commands and remaining integration work appear in [development documentation](docs/development/README.md). Historical and current [independent Claude review records](docs/reviews/README.md) preserve what was actually reviewed.
