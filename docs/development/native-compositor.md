# Experimental native compositor

An optional `fluxwm` frontend now joins the native driver to the existing Wayland window lifecycle, scene
composition, input handlers, layout and IPC. It publishes one selected physical output, renders into owned GBM
targets and publishes the matching immutable input scene only after the DRM flip completion. The main native
path is compiled and device-free tested; it has not been run on physical hardware.

## Build and explicit launch

```sh
cargo build -p fluxwm --features native --locked
./target/debug/fluxwm --backend native --native-output DEVICE:CONNECTOR --no-config
```

`DEVICE` is a decimal udev device ID and `CONNECTOR` a nonzero DRM connector ID from the native
inventory/session plan. This explicit launch changes physical display state and requires a separate authorized
physical VT and working seat broker. No native session is launched by the test suite. Default builds and
omitted backend selection still use nested mode; an unavailable native feature, missing target, invalid IDs
and native screenshot options fail before seat access. Ordinary `--exit-after-ms` and supervised client
arguments remain available. There is no installed display-manager session package.

The [reverting RGB probe](native-live-submit.md) remains a separate manual diagnostic. Main compositor mode is
a persistent opt-in session, not that timed preview.

## Rendering and lifecycle

The initial modeset must finish conclusively before output publication. The driver then advances an event
generation and drains the nonblocking DRM stream within a fixed read budget. Already-read batches keep their
old generation and cannot publish new frames; drain failure seals startup. These device-free checks do not
qualify cross-process restart on hardware; the [blocking-commit assumption](native-frame-transport.md)
remains a kernel/driver qualification requirement. Each dirty composition captures the
current window scene, imports its clipped elements, draws the cursor and returns the final GLES fence. The
native adapter waits before submitting the owned target. One pending frame throttles redraw; a matching
completion publishes its scene and sends advisory client frame callbacks. Those callbacks and DRM events do
not establish physical photon timing.

A single cancelled/replaced watchdog detects an unobserved pending flip after two event-loop seconds and stops
the session. It is not a deadline for rendering, kernel calls or shutdown. Seat loss, selected-GPU retirement,
transport errors and topology changes cancel held input, clear scene eligibility, remove the output and stop.
A topology change reports a distinct event and skips GPU re-preparation when retiring live scanout. Recovery
and multi-output transactions remain unimplemented; a retired output can remain unrestored. Retained frame
payloads also retain captured Wayland surface references until successful retirement or process exit. Shutdown attempts the driver's owned rollback
before loop destruction; cleanup/close failures fail the session. The [rollback ownership and process-exit
limits](native-live-submit.md) still apply.

IPC/readiness identify the selected backend. Native capabilities report a DRM flip clock and native-session
implementation, while `production_ready` remains false. This reports selected code paths, not hardware
qualification.

## Cursor policy and limits

Native mode paints a software arrow, honors hidden requests and imports client cursor surfaces with their
hotspots and output clipping. Named icons currently use the built-in arrow; theme lookup, named-icon shapes
and animation are unfinished. Pointer motion requests redraw only in native mode. The nested host cursor
behavior is unchanged.

Cursor validation stops traversal at 64 visited surfaces or depth 32, limits individual raw buffers and
logical destinations to 1024 pixels per dimension, raw pixels to 4,194,304 and local offsets to 16,384.
Invalid geometry or oversized trees hide the cursor. Oversized-tree warnings are limited to one per ten
seconds per context; these are implementation bounds, not
global client/GPU-memory or fairness guarantees. Extreme/nonfinite pointer and hotspot arithmetic is checked
before integer conversion. Cursor frames share the scanout target and final fence with window content.

## Remaining qualification

Pure tests cover option validation, lifecycle event policy, cursor coordinates and event-stream generations.
They do not execute the native context, drawing, cursor imports or watchdog against hardware.

Physical startup, input, frame orientation, cursor pixels, output rollback, timeout recovery and driver
compatibility remain unexecuted. There is no multi-output migration, automatic connector selection, VT
recovery, complete device policy, secure lock, DMA-BUF feedback/explicit-sync qualification or production
session packaging. Native frame allocation currently creates fresh targets and is not a qualified performance
design. See [production gaps](production-status-2026-10-18.md) and the [phase
gate](../phases/03-gpu-backend.md).
