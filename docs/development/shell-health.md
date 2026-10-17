# XDG shell client responsiveness

The nested runtime now observes XDG shell responsiveness through ping/pong. A newly bound `xdg_wm_base` receives one ping. A matching pong schedules the next probe five seconds after the pong is processed. An outstanding probe has a two-second monotonic deadline. There is at most one outstanding ping per monitored binding, and one shared calloop timer tracks the earliest due probe or deadline. No monitor timer runs when all bindings are gone or every outstanding probe is already timed out.

A missed deadline changes diagnostics to `unresponsive`; it never kills the client, drops its buffers, clears input ownership or posts an unresponsive protocol error. A late matching pong recovers to `responsive` and schedules the next probe. Wrong/replayed serials do not satisfy the probe: the pinned Smithay handler checks the serial before invoking the compositor callback. A pong processed at/after its deadline records both the missed deadline and recovery, even if the timer callback has not yet run.

These labels describe the shell event path only. A client can answer pings while its application or frame production is stalled. Timeout includes outgoing event backpressure and server scheduling delays; it is not proof of a crashed client or an application latency measurement. The two/five-second constants are initial nested development policy, not a qualified production responsiveness budget. Suspend/resume/native and overloaded-server policy remain open.

## Diagnostics and ownership

`windows.list` includes `responsiveness: {id, status}` for an XDG toplevel. IDs identify the shell binding and are decimal strings, never reused within the monitor's lifetime. Multiple bindings on one Wayland connection have independent probes. Destroying one binding removes only that binding. Role unmap does not destroy its shell binding or stop its monitoring.

`diagnostics.shell_health` reports `timeout_ms`, `interval_ms`, `capacity`, `timer_armed`, bounded client entries and cumulative `sent`, `pongs`, `timeouts`, `recoveries`, `send_failures`, `capacity_rejections`, and `timer_fired` counters. Status is `unknown` before the first probe, `waiting` while pending, `responsive` after a valid pong, `unresponsive` after timeout, or `unavailable` after a ping-send failure. A binding omitted from the monitor has `unmonitored` status in window queries. Observable transitions advance the IPC state sequence before queries are dispatched. Timer/pong handling requests no scene redraw.

The monitor caps its own records at 1,024 shell bindings. Excess resources remain valid and usable but receive no automatic monitoring; capacity rejection is explicit and a role using an excess binding reports `unmonitored`. This bounds this monitor's vector, not all Wayland resources, client admission, aggregate memory, upstream dispatch, or total event-loop work. It does not claim hostile-client resource isolation. Scans are bounded by those records, but worst-case latency still needs measurement.

Only a weak loop handle is retained; teardown removes its single timer. Explicit binding destruction and disconnect remove records, and polling additionally prunes dead handles. IPC shutdown clears monitor work while draining. Signal/host-close/fatal teardown removes it with state. This is not the full bounded native shutdown protocol.

## Reproduce

```sh
cargo build -p fluxwm -p fluxwm-fixture
python3 tools/smoke_private.py --suites health
```

The [authenticated private display wrapper](shutdown-smoke.md) runs `tools/smoke_health.py` without host input injection. `--bundle PATH` can reuse the previously verified Xvfb package cache. Each health case creates a unique compositor/fixture instance with separate logs, a 20-second case deadline and explicit executable/fixture hashes. The client has a 12-second limit; no Lua or external application is launched. Final evidence is in [the shell-health bundle](../evidence/nested-milestone/2026-10-17-shell-health/README.md).

| Real fixture | Required observations |
| --- | --- |
| `shell-health` | Two independent bindings on one connection; wrong serial leaves the first waiting while the second is responsive; no further Wayland requests during 2.2-second timeout observation; real timer fires and IPC sequence advances; second binding maps and gets a frame while first is unresponsive; late matching pong recovers; replay ignored; later probes use fresh serials and valid pongs; destroying bindings removes records and timer |
| `shell-health-capacity` | Bind 1,026 resources; exactly 1,024 monitored and two capacity rejections; an overflow role reports unmonitored yet maps and gets a frame; explicit destruction retires all records and timer |

The first real run confirmed the serial/timeout observations but failed later because the fixture omitted the toplevel's initial bufferless commit. That fixture error was corrected and retained as failed evidence. The next run passed both scenarios: timeout sequence 3→4, late recovery and subsequent probing, then empty monitor/timer; the capacity run completed in 91 ms locally. Those timings and counts refer to that run, not a hardware or maximum-RSS guarantee.

Three focused tests cover exact deadline boundaries, once-only timeout, late-pong recovery before timer servicing, duplicate-pong behavior and empty monitor/drop ownership. Synthetic time tests supplement real protocol traces; they do not qualify every dispatch race. Multiple connections, client destruction exactly at a deadline, serial wrap, long backpressure, repeated signals, process/device/session policy and full production phase gates remain separate work.
