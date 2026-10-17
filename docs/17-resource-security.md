# Resource budgets and privilege enforcement

All numeric limits here are **proposed starting values**, not measured safe defaults or upstream protocol mandates. Phase owners must validate legitimate application workloads, expose effective limits in diagnostics, and record revisions. Never advertise an arbitrary silent limit as guaranteed protocol behavior.

## Trust model

Baseline model A distinguishes ordinary app protocol rights from trusted unsandboxed host-session authority. Tagged sandbox connections are restricted; unsandboxed authenticated same-user host processes are trusted local-session principals, consistently with IPC/config access. Host user sessions commonly permit same-user processes to control broader resources outside Wayland; FluxWM cannot promise protection from every compromised host process. Sandbox contexts add metadata and policy hooks, not an OS-wide security boundary by themselves.

| Principal | Allowed baseline | Privileged capability policy |
| --- | --- | --- |
| Ordinary application role | Own surfaces/input/data offers | Tagged sandbox gets no privileged globals; unsandboxed same-user host connections may bind listed privileged families under model A |
| Sandbox-context client | Baseline plus explicitly granted integration | Deny nested security-context delegation; no self-granted privileged role |
| Unsandboxed host desktop helper | Compatible normal session socket, including daemon activation | Listed privileged protocols available; per-object/lock/source rules still enforced |
| Portal backend | Normal trusted host activation plus portal session grant | Validate session owner/scope and revocation; host raw capture tools remain within trusted-host boundary |
| Lock helper | Current selected lock object/surfaces and authenticated unlock | Not authorized by app title/app ID; replacement has explicit handoff |
| Accessibility helper | Trusted-host monitoring/actions needed for assistive stack | No compositor-wide lock/password key stream; application/lock toolkit semantics qualified separately |
| fluxctl/local IPC | Same-user local control per documented policy | Stronger sandbox isolation requires an additional channel policy |

Use authenticated peer/session and trusted sandbox-context provenance for baseline classification, never app ID/title/PID-name matching. Daemon-started host helpers need no inherited-fd-only path. Optional inherited private connections can harden particular roles. Model A does not claim protection against compromised unsandboxed same-user software or host filesystem/IPC access granted to a sandbox. See the [selected trust decision](21-review-resolutions.md).

`can_view`/registry filtering is only the first boundary. Once a client has bound an object, hiding the global does not automatically revoke existing object requests. Each sensitive operation needs current authorization checks, source generation, and lock/session permission epoch. On revoke, stop active sessions and drain safe completion paths. Security contexts cannot be nested to claim a stronger identity, and context listener shutdown must follow source protocol lifetimes.

## Initial bounded-resource policy

| Resource | Proposed ceiling / policy | Over-limit response and validation |
| --- | --- | --- |
| IPC frame | 1 MiB including JSON payload, 4-byte big-endian unsigned length | Reject before payload allocation; framing max excludes prefix explicitly |
| IPC connections | 32 per compositor instance initially | Refuse excess with clear diagnostic; test reconnect storms |
| IPC subscriber backlog | 256 events or 1 MiB, whichever first | Disconnect/resync policy with state sequence; never block producers |
| Pending commands | 4,096 globally, 256 per source per dispatch batch | Coalesce pan/query where safe; preserve lifecycle/lock priority; disconnect abuser as policy permits |
| Config source | 1 MiB per file, 4 MiB total modules | Reject before evaluation; bound module count and cycles |
| Config VM allocation | 32 MiB VM limit plus parent-enforced process limit evaluated on target OS | Distinguish Lua allocator accounting from process RSS and native allocations |
| Config evaluation | 1 million instructions plus 1-second parent wall deadline | Kill/reap helper on deadline; one active helper plus one latest request |
| Config result | 4 MiB payload, depth 32, 100,000 visited values | Raw traversal; reject cycles/metatables/functions/userdata/threads and sparse arrays |
| Config pattern | 4 KiB per pattern; bounded engine; 1,024 rules | Compile in parent background work; no catastrophic-backtracking implementation |
| Unretired render snapshots | 32 per output and an explicit bytes budget | Stop optional closing animations before retaining more; never drop live GPU leases |
| Thumbnail/effect cache | Start 128 MiB global at low-memory profile, scaled profiles separate | LRU eviction of completed resources only; primary render targets outside optional cache budget |
| Capture | 4 sessions, up to 3 in-flight consumer buffers each as negotiated | Drop obsolete unsent work or throttle; never hold presentation waiting for consumer |
| Clipboard/DnD transfers | 32 active routes, no compositor-wide eager payload buffering | Nonblocking fds, inactivity timeout with documented exceptions, cleanup on owner death |
| Client-owned object growth | Accounting per object class; thresholds to be established with toolkit traces in phase 2 | B11 stays open until measured policy exists; no invented universal Wayland object maximum |

These are admission/scheduling controls, not guaranteed memory maxima. Imported GPU buffers, driver allocations, client memory, stack/native library allocation, SHM virtual mappings, and kernel objects must be measured separately. A 32 MiB Lua limit cannot bound the whole compositor or helper RSS. Helper process limits must preserve enough headroom for startup and error reporting.

## Config process containment

Use a dedicated `fluxwm-config` helper process or equivalent subcommand supervised by the parent. Pass a bounded input snapshot and explicit environment. Restrict file/module access and native functions. Before exec, explicitly close every fd outside the helper allowlist (using a qualified close-range/spawn mechanism, not only trusting library close-on-exec flags). Parent starts a monotonic deadline, retains a child handle, reads result nonblockingly, and reaps on success/failure/timeout/shutdown. A killed helper's result cannot be applied later. Use robust child identity/handles instead of signaling an unrelated reused PID.

The [Lua probe](evidence/lua54-probe.txt) proves ordinary hook errors are catchable under the tested Lua 5.4.9. The standalone probe is not an mlua test. A separate [pinned mlua probe](evidence/probes/mlua-hook/README.md) built successfully and caught five hook errors too. Neither proves every possible budget mechanism is bypassable or validates a production supervisor. Still, a hard parent deadline avoids depending on catchability, coroutine inheritance, or bounded native execution. Implement the actual helper stress tests in phase 6.

Accept only text Lua chunks by default. Use raw table traversal without invoking `__index`, `__pairs`, `__len`, conversion metamethods, or user callbacks. Maintain visited identity and depth counters. Deep or cyclic tables are invalid config. Retain pure functions only during evaluation; returned schema must be plain serializable data. Validate UTF-8 where required; reject embedded NUL in OS argv/path fields and reject unknown enum/action values.

For modules, open files through a constrained directory handle and explicit path policy, handle symlinks deliberately, take a consistent bounded snapshot, and watch all included files. This policy must be implemented and tested on supported platforms; a string prefix check is insufficient. Do not expose secrets merely because they are readable by the same user. Config files remain user-trusted to select spawn/autostart commands, so parser isolation does not make malicious user config harmless after accepted commands are applied.

## Fair scheduling and exhaustion

Dispatch limits must preserve transport draining and protocol progress without letting an abusive fd monopolize the loop. Reserve capacity for lock/session/device events and resource retirement. Backpressure counts bytes and object counts; a thousand tiny events and one huge allocation are different risks. Per-client throttling cannot delay GPU completions that free global resources.

On `EMFILE`, allocation failure, oversized texture, shader compile failure, or helper fork failure: diagnose once with rate limiting, reject the current optional operation, and preserve already valid state where possible. Rust allocation failure may abort depending on allocation path and build policy; document realistic recoverability instead of promising every OOM is catchable. Keep optional diagnostics allocations bounded and reserve a minimal failure message path.

Avoid spawning one worker per failed reload/capture; abandoned threads are not reclaimed by ignoring results. Avoid unbounded retries against lost DRM devices. Track oldest outstanding lease and growing object class to distinguish actual leaks from intentional caches. A maximum-age alarm is diagnostic, not permission to free GPU memory still in use.

## Security release gate

Review unsafe/FFI additions, privileged global registrations, IPC commands, config native bindings, source/capture permission changes, and helper environment/fd inheritance. Fuzz client lifecycle and serialization boundaries. Re-run lock/capture/input privacy cases after renderer/plane optimizations, since a new fast path can bypass an otherwise correct policy. Audit logs record principal class and decision reason without keystrokes, clipboard data, or sensitive titles.

The exact helper spawn/fd/environment/limits and Lua allowlist are specified in [review resolutions](21-review-resolutions.md). Host config/IPC policy is deliberately the same trust boundary as unsandboxed host Wayland access; `portal_capture = consent` is not a universal raw-capture consent guarantee. Lock restrictions remain invariant for supported compositor APIs.
