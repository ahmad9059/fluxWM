# FluxWM

Design and implementation documentation for a Rust Wayland compositor with an infinite scrolling canvas, GPU rendering, expressive animations, and Lua configuration.

Start with the [documentation index](docs/README.md) and the [ten-phase implementation roadmap](docs/10-roadmap.md).

**Project status:** research and planning only. There is no compositor implementation, compositor Cargo workspace, working configuration parser, or measured compositor performance result yet. The documentation includes a small standalone Rust/mlua evidence probe, separate from future compositor code. All FluxWM APIs, commands, structures, and numerical targets in these documents are proposals unless explicitly identified as upstream facts.

The [deeper production audit](docs/13-production-audit.md) adds immutable source evidence, blocking decisions, state machines, 94 edge cases, security/resource limits, and release contracts. Run documentation checks with `python3 tools/check_docs.py`. See [independent review records](docs/reviews/README.md) for Claude’s findings and their disposition.
