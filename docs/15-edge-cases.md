# Adversarial and edge-case catalog

**112 specified cases plus explicitly narrower early variants; no FluxWM runtime passes.** The [JSON catalog](evidence/edge-cases.json) and [phase-gate mapping](evidence/phase-gates.json) are authoritative for scheduling. `introduced_phase` starts design/harness work; `qualified_phase` is the earliest full-case exit gate. Every required feature must exist by qualification. An early variant never passes the later full case.

All statuses remain `specified`. Execute disruptive device/crash/resource tests only in isolated supported test sessions. See [review resolutions](21-review-resolutions.md) for precise policy decisions and [window-state matrix](22-window-state-matrix.md).

## SURF cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| SURF-01 | 2 / 2 | Ack a stale/unknown configure serial | Protocol-defined handling; no rollback to an obsolete target | Synthetic client protocol trace + target epoch |
| SURF-02 | 2 / 2 | Commit null buffer then remap same role | One membership removed then one restored; no duplicate tile | Model membership and map/unmap events |
| SURF-03 | 2 / 2 | Destroy parent with nested popup grabs | Dependent grabs/surfaces cleanly retired; normal focus recoverable | Seat/grab log + client trace |
| SURF-04 | 2 / 2 | Commit synchronized and desynchronized subsurfaces in varying order | Only valid committed surface-tree state appears | Reference image + commit tree trace |
| SURF-05 | 2 / 2 | Reuse buffer storage across multiple surfaces/outputs | No release while any real consumer still reads it | Buffer lease/fence trace |
| SURF-06 | 2 / 2 | Shrink/truncate an SHM pool backing file while mapped | Invalid memory access cannot kill compositor; library fault policy exercised | Isolated subprocess client test and server liveness |
| SURF-07 | 2 / 2 | Overflow dimensions/offset/stride arithmetic | Reject invalid allocation/import before unsafe access | Boundary-value client + checked failure result |
| SURF-08 | 2 / 2 | Commit a valid buffer with empty input/opaque region | Correct rendering and click-through/occlusion semantics | Image + hit-test trace |
| SURF-09 | 4 / 8 | Exercise every window-state matrix row at supported interface versions | Configured states/caps/bounds match matrix; unsupported version fields absent; hidden state restores | Synthetic configure/state traces and version bindings |
| SURF-09:strip-native | variant / 7 | Narrow early variant | Native and selected X11 strip/floating/maximize/fullscreen/minimize states obey versioned matrix; canvas/overview rows excluded | Same class of trace, limited to listed features |

## LAY cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| LAY-01 | 4 / 4 | Open next column with viewport already full | Unrelated columns retain widths; camera reveals new focus | Before/after rectangles + camera trace |
| LAY-02 | 4 / 4 | Client minimum heights exceed stack available height | Keep each minimum, expose vertical overflow scroll inside stack when minima exceed height; finite nonnegative rectangles and every member reachable | Constraint solver state + visible recovery |
| LAY-03 | 4 / 4 | Close focused last tile while camera moving | Focus becomes live neighbor or empty; no dangling animation target | Fake-clock command replay |
| LAY-04 | 4 / 8 | Pan canvas to huge positive/negative positions then rebase | Relative geometry and drag anchors stay stable within tolerance | Transform round-trip and scene comparisons |
| LAY-05 | 4 / 8 | Switch strip/canvas repeatedly with new and destroyed windows | Unique membership; mode state reconciled deterministically | Property trace across both saved states |
| LAY-06 | 4 / 4 | Make usable area zero with panel reservations | No NaN/division by zero; retain recoverable layout | Layer reservation log + finite-state assertions |
| LAY-07 | 4 / 4 | Create equal-distance directional candidates | Select by angular deviation, edge projection, perpendicular distance, then smallest stable generational ID | Recorded candidate scores and selected ID across repeated replay |
| LAY-08 | 4 / 4 | Transfer fullscreen constrained window between scales | Restore geometry remains valid and panels follow policy | Target rect/scale/fullscreen state + images |
| LAY-09 | 4 / 4 | Fullscreen client submits smaller transparent content during focus transition | Centered content with opaque independent backdrop; no unrelated content under fullscreen tree | Alpha image fixtures and fullscreen state/transition trace |

## GPU cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| GPU-01 | 3 / 9 | Acquire point never signals for newly attached buffer | Do not read; unrelated outputs/input continue | GPU acquire trace + independent frame evidence |
| GPU-01:implicit | variant / 3 | Narrow early variant | Delay backend-supported implicit buffer readiness; no reader samples it early; independent input progresses | Same class of trace, limited to listed features |
| GPU-02 | 3 / 9 | Layout deadline expires before GPU acquire | Fallback uses previous safe content; deadline never bypasses acquire | Transaction and buffer IDs correlated |
| GPU-02:implicit | variant / 4 | Narrow early variant | Layout deadline chooses previous acquired content; no read of pending implicit buffer | Same class of trace, limited to listed features |
| GPU-03 | 3 / 9 | Release point timeline equals acquire with invalid ordering | Protocol error at required validation point | Synthetic syncobj client trace |
| GPU-04 | 3 / 9 | Replace a never-read explicit-sync commit | After replacement processing, release is signaled by next retirement dispatch; reader/import count is zero | Timeline events + dispatch sequence + consumer ledger |
| GPU-04:implicit | variant / 3 | Narrow early variant | Replace never-read implicit buffer; zero readers/imports and release in next retirement dispatch | Same class of trace, limited to listed features |
| GPU-05 | 3 / 9 | Remove direct-scanned buffer from scene | Hold through replacement/disable completion | KMS commit/page-flip + lease retirement |
| GPU-05:primary-plane | variant / 3 | Narrow early variant | Compositor-owned primary framebuffer retained through replacement/disable completion; no client direct-scanout support assumed | Same class of trace, limited to listed features |
| GPU-06 | 3 / 9 | Render on GPU A and scan/capture on GPU B | Every consumer synchronization respected; explicit copy if needed | Per-device fence/import/copy ledger |
| GPU-07 | 3 / 9 | Destroy wl_buffer while explicit-sync GPU read is deliberately delayed | Source ownership survives resource destruction; explicit release timeline signal is not earlier than final GPU completion | Correlated GPU completion/release timestamps and retained owner count |
| GPU-07:implicit | variant / 3 | Narrow early variant | Delay GPU completion; final source ownership remains until completion even after wl_buffer destruction | Same class of trace, limited to listed features |
| GPU-08 | 3 / 9 | GPU reset/import failure during blur and capture | No stale handle reuse; explicit recovery or controlled session failure | Injected failure + epoch/resource diagnostics |
| GPU-09 | 3 / 9 | Attach SHM buffer to explicit-sync surface on selected Smithay path | Client receives unsupported-buffer protocol error; server and other clients survive | Synthetic request/error trace and server liveness |

## OUT cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| OUT-01 | 3 / 7 | Remove output with queued page flip and capture | Reject stale output epoch; safely retire old work | Epoch/commit/capture trace |
| OUT-01:no-capture | variant / 3 | Narrow early variant | Remove output with queued page flip; stale epoch ignored and prior resources retire safely | Same class of trace, limited to listed features |
| OUT-02 | 3 / 3 | Disconnect every output then reconnect one | Server stays alive, no output frames submitted while none exist, all windows enumerated, focused live window restored on reconnect | Socket liveness, window IDs before/after, submission count and focus |
| OUT-03 | 3 / 3 | Two displays expose identical/missing EDID | No incorrect claim of unique identity; deterministic safe placement | Identity matching diagnostics |
| OUT-04 | 3 / 3 | Preview unsupported mode then let confirmation time out | Keep or restore usable state; do not replay disconnected handles | Atomic test/apply/revert record |
| OUT-04:cli-preview | variant / 3 | Narrow early variant | CLI-acknowledged mode preview reverts after 15 seconds without acknowledgement; no text UI assumed | Same class of trace, limited to listed features |
| OUT-05 | 3 / 3 | Switch VT repeatedly while clients animate | No device use while session inactive; resume repaints safely | Session/backend event trace |
| OUT-06 | 3 / 7 | Suspend locked while external output powers off | No sensitive frame on resume; lock remains authoritative | Physical test video + lock/presentation trace |
| OUT-07 | 3 / 5 | Change 60/144 Hz pair to rotated fractional output | Independent clocks; pointer and physical edges agree | Presentation times + geometry/picking trace |
| OUT-08 | 3 / 3 | Hotplug storm from MST dock | Bound probing, invalidate removal immediately, no fd/resource growth | Stress event log + fd/memory trend |
| OUT-09 | 3 / 7 | Request lock with output powered off, then wake/suspend | Lock causes no output wake; disabled/plane-cleared state is confirmed; first framebuffer after later wake is lock or black; unprovable backend path fails B08 | Atomic state/plane completion, no-wake event log, first-wake framebuffer and physical observation |

## INP cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| INP-01 | 5 / 5 | Consume key press then change focus before release | Consistent ownership; no accidental client action/stuck shortcut | Seat event trace |
| INP-02 | 5 / 7 | Enter lock/unlock while shortcut modifiers held | No ordinary shortcut leaks; fresh chord policy enforced | Physical events + command log |
| INP-03 | 5 / 5 | Cancel gesture after compositor/client ownership decision | Exactly one owner sees valid end/cancel | Gesture and client protocol traces |
| INP-04 | 5 / 7 | Close constrained-pointer surface mid-drag | Constraint/grab released; pointer recoverable | Relative/constraint state trace |
| INP-05 | 5 / 8 | Change zoom during world-space group drag | Grab anchor stays under pointer | Inverse-transform numerical checks |
| INP-06 | 5 / 5 | Retarget spring under irregular 30–240 Hz sampling | Continuous position/velocity and eventual settle | Fake-clock curve outputs |
| INP-07 | 5 / 7 | Remove touch/tablet device with active contacts | Cancel contacts/tools correctly; no ghost interaction | Device and seat lifecycle traces |
| INP-07:pointer-only | variant / 5 | Narrow early variant | Remove pointer device mid-button drag; release/cancel ownership and keep keyboard recovery | Same class of trace, limited to listed features |
| INP-08 | 5 / 5 | Reconfigure keymap/layout while keys held | Cancel compositor-owned chords, publish coherent new keymap/modifiers, suppress shortcut retrigger until held keys released; no stuck client modifiers | xkb/seat state and client events |

## CFG cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| CFG-01 | 6 / 6 | Catch instruction-hook error repeatedly with pcall | Parent deadline kills helper; compositor responsive | Probe + actual helper supervisor test |
| CFG-02 | 6 / 6 | Return cyclic/metatable/very deep Lua tables | Bounded raw conversion rejects unsupported structure | Field-path diagnostic + memory bound |
| CFG-03 | 6 / 6 | Use sparse/mixed-key action arrays or embedded NUL argv | Reject ambiguous schema or invalid OS argument | Schema boundary test |
| CFG-04 | 6 / 6 | Rename config during read or traverse module symlink | Consistent snapshot; no unintended file disclosure | Open/read trace + path-policy test |
| CFG-05 | 6 / 6 | Reload newer generation while older helper finishes | Stale result discarded; one active generation | Helper IDs and config events |
| CFG-06 | 6 / 6 | Send partial/oversized/duplicate-ID IPC requests | Bounded parser, deterministic error, no unrelated client impact | Raw framing test + socket telemetry |
| CFG-07 | 6 / 6 | Slow IPC subscriber never drains | Bounded output with disconnect/resync; render loop never waits | Queue size and responsiveness trace |
| CFG-08 | 6 / 6 | Apply output setting fails after logical validation | Desired/effective state reported honestly; preserve working session | Config/output transaction record |

## SEC cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| SEC-01 | 7 / 7 | Two lock clients race then loser creates surface | Only selected owner has authority; loser cannot cover/unlock | Client identities and lock-object trace |
| SEC-02 | 7 / 7 | Locker dies before/after lock confirmation | Correct reject or persistent locked recovery; no false success | Lock state + presented safe-frame evidence |
| SEC-03 | 7 / 7 | Hotplug output with cursor/overlay planes while locked | First visible content is safe; no stale plane leaks | Plane assignment and physical observation |
| SEC-04 | 7 / 7 | Sandbox client spoofs app_id/title/UID-derived role | Metadata does not grant capture/lock/input privilege | Authorization audit decision |
| SEC-05 | 7 / 7 | Hide/remove privileged global after client already bound | Existing object actions still checked; revocation not just registry change | Bound-object request test |
| SEC-06 | 7 / 7 | Nest security contexts or disconnect creating launcher | No privilege escalation; listener lifetime follows protocol | Listener fd lifecycle and context trace |
| SEC-07 | 7 / 7 | Accessibility/input-method helper requests keys while locked | Trusted host IME/virtual keyboard may target only current lock tree; normal applications and compositor key-monitor streams receive no lock input | D-Bus/Wayland access decision + input trace |
| SEC-08 | 7 / 10 | SIGKILL compositor while locked in supported isolated display-manager session | Session manager returns only safe greeter/blank/VT and never an automatically restarted unlocked desktop; route fails qualification otherwise | Physical/KMS observation through crash and session-manager journal |
| SEC-09 | 2 / 7 | Idle daemon starts ordinary session locker | Trusted host transport permitted; only winning lock object gains current authority | Peer/context and lock object trace |
| SEC-10 | 2 / 7 | Portal backend activated by D-Bus/systemd | Trusted host activation works without inherited-fd prerequisite; selected grant/source enforced | Activation log, peer class, chooser/browser share |
| SEC-11 | 2 / 7 | Tagged sandbox requests privileged globals/operations | No default capture/input/lock/output/data-control authority; untrusted app_id cannot promote it | Registry and existing-bound-object denial traces |
| SEC-12 | 2 / 7 | Trusted host edits policy config and sends IPC mutation | Applies same trusted-host boundary and immutable lock invariants; no claim of protection against host home/IPC access | Config diff/IPC principal and effective security state |
| SEC-12:host-control | variant / 6 | Narrow early variant | Same-user host config/IPC mutation succeeds only through validated commands; no lock implementation claim in this variant | Same class of trace, limited to listed features |
| SEC-13 | 3 / 7 | Enter non-Latin password through lock IME and touchscreen OSK | Any authenticated trusted-host IME/OSK follows current lock focus; tagged sandbox injection denied; password context absent from capture/global compositor key monitors | Lock IME/OSK and capture/observer traces |

## CAP cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| CAP-01 | 7 / 7 | Revoke capture permission with queued/in-flight copies | No sensitive job submitted after revocation epoch; no stale ready/portal publication; already issued consumer memory explicitly outside retroactive revocation | Permission epoch vs job submit/completion/publication trace |
| CAP-02 | 7 / 7 | Close window being shared | Stop selected source; never silently share whole output | Portal/session/source events |
| CAP-03 | 7 / 7 | Reuse PipeWire node ID or reconnect monitor | Opaque source identity prevents substitution | Node serial/session mapping |
| CAP-04 | 7 / 9 | Mark window private during blur/direct scanout/overview | Every capture source honors policy without leaking sampled pixels | Output/window capture images and effect dependencies |
| CAP-05 | 7 / 7 | Restore token after permission revoked or target missing | No stale grant; reprompt or fail coherently | Portal persistence response and chooser evidence |
| CAP-06 | 7 / 7 | Large clipboard transfer receiver stalls/dies | No event-loop blockage; bounded fd/transfer lifetime | Async pipe backpressure test |
| CAP-07 | 7 / 7 | Kill selected X11 bridge while X11 clients are active | Report connection loss, retire X11 windows/selection ownership, clean helper/socket state, allow a new display instance without claiming old apps survived | Helper lifecycle, old/new connection IDs and window/resource counts |
| CAP-08 | 7 / 7 | IME popup over transformed window then focus changes | Correct placement, text routing, surrounding-text privacy | GTK/Qt IME session trace |
| CAP-09 | 7 / 7 | Destroy capture destination during delayed copy | Retain destination backing until GPU write completes; no ready/message to destroyed object | Destination lease/fence and resource destruction trace |
| CAP-10 | 7 / 7 | Lock then unlock an active browser share | Same source/session emits black without cursor while locked; resumes only after valid grant recheck; no terminal stopped on lock | Session IDs, frame pixels, GPU completion and permission epochs |
| CAP-11 | 7 / 9 | Share while private IME, notification, thumbnail and magnifier content exists | Capture graph inherits privacy to associated content; window capture samples no desktop background | Redacted scene/effect graph plus output/window frame pixels |
| CAP-12 | 7 / 7 | Request new capture session/portal grant while locked | Deny new session/grant without exporting lock UI or source metadata; existing preauthorized sessions continue black | Protocol/portal denial plus existing-session IDs and black frames |

## A11Y cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| A11Y-01 | 8 / 8 | Screen reader enters overview/search and returns to app | Semantic nodes, focus and selected state announced | Orca + accessible tree recording |
| A11Y-02 | 5 / 8 | RTL/combining/emoji title in search and task list | Safe shaped display; search and clipping do not corrupt UTF-8 | Text fixture images and semantic labels |
| A11Y-02:text-fixture | variant / 5 | Narrow early variant | Shape and render bidi/combining/emoji fixture with declared fallback and grapheme-safe truncation; no search UI required | Same class of trace, limited to listed features |
| A11Y-03 | 5 / 5 | Malformed or bidi-control-heavy client title | No terminal escape injection or spoofed diagnostic authority | Sanitized logs and bounded UI text |
| A11Y-04 | 8 / 8 | Accessibility adapter blocks/disconnects | Main loop remains responsive; explicit status, safe restart | Worker queue/backpressure fault test |
| A11Y-05 | 8 / 8 | Nested compositor starts alongside accessible host | No takeover of host keyboard-monitor/session bus ownership | D-Bus names and input-routing evidence |
| A11Y-06 | 8 / 8 | Enable magnification/reduced motion/high contrast | All core actions remain reachable and correctly picked | Keyboard-only journey + transformed picking |
| A11Y-07 | 8 / 8 | Accessible lock/chooser is unavailable | Supported session reports blocker; inaccessible fallback not called complete | Companion dependency and actual assistive test |
| A11Y-08 | 8 / 8 | Lose every output in accessible full session | Announce no displays, keep server/task enumeration, give no app input focus until display returns, then restore discoverable UI | Assistive announcement + IPC membership + focus/presentation trace |
| A11Y-09 | 3 / 5 | Minimal install with missing preferred font and emoji title | Declared fallback renders legible text or explicit supported fallback; bounded per-scale glyph cache | Font inventory, reference images, glyph memory counters |

## OPS cases

| ID | Introduce / qualify | Stimulus | Expected result | Evidence |
| --- | --- | --- | --- | --- |
| OPS-01 | 10 / 10 | 24-hour churn with spawn/close/reload/capture | No unbounded fd/memory/GPU resource growth after steady-state accounting | Raw time series and categorized resource owners |
| OPS-02 | 10 / 10 | OOM/fd exhaustion during output creation | Fail with explicit bounded fallback; retain safe session if possible | Isolated resource-limit test |
| OPS-03 | 10 / 10 | Package upgrade changes config/IPC schema | Old config diagnosed/migrated; incompatible client rejected clearly | Clean upgrade/rollback test |
| OPS-04 | 10 / 10 | User service environment shared with nested/session processes | No host activation variable overwrite from nested mode | Environment import log and service ownership |
| OPS-05 | 10 / 10 | Shutdown with unreaped helpers and pending frames | Bounded orderly cleanup; only supervised children terminated | Process/fd/socket lifecycle evidence |
| OPS-06 | 10 / 10 | Clean install missing portal/lock/auth companion | Dependency preflight names missing contract; no false readiness | Fresh distribution install report |
| OPS-07 | 10 / 10 | Benchmark under thermal throttling/background load | Record conditions/variance; no cherry-picked optimization claim | Repeated raw benchmark artifacts |
| OPS-08 | 10 / 10 | Dependency update adds protocol global/new unsafe path | Inventory diff reviewed; affected gates rerun | Source/feature diff and targeted qualification |

## Result promotion

A future pass needs exact FluxWM build, environment, procedure, observed assertion, raw artifacts and reviewer/date. `documented behavior` alone is not an oracle. Failure, blocked and not-applicable results need explicit reasons. The [checker](../tools/check_docs.py) validates feature/phase coherence and Markdown/JSON row agreement, not runtime correctness.

## Additional production-audit cases

Added after source inspection of the initial implementation. These extend the original 94 cases; none has a runtime pass. The original historical review reports retain their original counts. RUN-06/07 describe reproduced IPC defects; a failing probe is evidence of a gap, not a qualification pass.

| Case | Introduced / qualified phase | Stimulus | Required result | Evidence required |
| --- | --- | --- | --- | --- |
| RUN-01 | 2 / 2 | One Wayland client keeps its socket readable while a second maps and closes a window | Bound work per dispatch and service the second client and shutdown source within a declared measured dispatch-latency budget | Two synthetic clients; per-source dispatch counts, monotonic latency distribution, chosen budget and raw trace |
| RUN-02 | 2 / 3 | Leave a static nested scene idle, then commit damage and request one frame callback | No self-sustaining redraw loop; commit wakes renderer and requested callback progresses without continuous polling | Idle redraw/submit counters followed by commit-to-frame trace; CPU samples are supporting evidence |
| RUN-03 | 2 / 3 | Resize nested host to zero dimensions, minimize, then restore while a client commits | No zero-sized EGL submit or unwrap failure; retain membership and repaint latest committed content on restoration | Host-size event log, submission sizes, client IDs and restored image |
| RUN-04 | 2 / 3 | Request shutdown during partial client input and while a supervised helper is starting | Wake event loop, stop owned work, reap supervised children and remove only owned sockets; do not affect host services | Isolated process-tree and socket inode snapshots before/after SIGTERM and normal exit |
| RUN-05 | 3 / 5 | Advance monotonic animation time across a long suspend-like gap and repeated frame timestamp values | Retarget deterministically, bound integration work and settle according to resume policy without NaN or timer storm | Fake-clock replay and frame dispatch counts; native suspend is separately qualified by OUT cases |
| RUN-06 | 2 / 6 | Send a valid IPC request then shut down only the client write half | Process complete buffered request and flush its response before retiring the connection; reject incomplete trailing frame | UnixStream shutdown-write integration test and decoded response transcript |
| RUN-07 | 2 / 6 | Mutate state then send malformed or unsupported request in the same IPC batch | Every later response uses a coherent nonregressing current state sequence, including transport-generated errors | Decoded ordered response sequence trace with mutation handler counter |
| RUN-08 | 2 / 6 | Server trickles a framed reply and config source is a FIFO or keeps growing | CLI transaction and config source acquisition obey total monotonic deadlines and byte bounds; compositor dispatch never waits for either | Isolated trickle-server and FIFO fixtures, wall elapsed times, fd/process cleanup evidence |
| SURF-10 | 2 / 2 | Bind xdg-shell capability-event version and exercise every advertised toplevel capability | Advertise only implemented behavior; unsupported requests preserve valid protocol progress without fake success | Synthetic client capability event and request/configure/state transcript |
| SURF-11 | 2 / 2 | Request move or resize after unmap or destruction and destroy drag source during callbacks | Validate live seat/client/serial and membership; cancel safely without process panic or stale focus | Synthetic client and adversarial callback-order replay with server exit status and seat trace |
| SURF-12 | 2 / 7 | Issue activation token after focus changes, reconnect, expiry or client destruction | Apply declared freshness and user-intent policy; metadata alone cannot steal focus and stale tokens cannot target reused identities | Token lifecycle and seat serial traces with denied/allowed outcomes across GTK/Qt/native fixtures |
| INP-09 | 3 / 5 | Run pointer motion through rotated mixed-scale outputs with negative origins and a confined surface | Use one coherent coordinate conversion; preserve pointer position across boundary and respect input regions and constraint lifetime | Physical/logical/surface transform trace and independently computed picking fixtures |
| INP-10 | 3 / 5 | Send high-resolution wheel fractions, finger stop and direction changes while camera animates | Preserve supported axis precision/source/stop semantics and route each gesture to one owner without a stuck scroll | Synthetic axis event transcript and camera-versus-client delivery counts |
| INP-11 | 3 / 7 | Change idle inhibitors while windows hide, outputs power off and session locks | Apply visibility and lock policy per inhibitor type; stale/destroyed surface cannot inhibit indefinitely or bypass lock | Idle timer/inhibitor ownership trace, DPMS and lock transition recording |
| GPU-10 | 3 / 4 | Commit odd-sized buffers with fractional scale, viewport crop, transform and nonzero window geometry | Validate coordinate domains and legal buffer state; shared edges remain consistent and picking matches rendered content | Pixel reference fixtures plus surface/buffer/output transform and input-region trace |
| GPU-11 | 3 / 9 | Reject atomic test or real commit during direct-scanout entry, exit or output reconfiguration | Preserve last confirmed state and resource ownership; safely compose fallback or fail the declared route without unsafe release | Fault-injected KMS commit results, plane/framebuffer ownership ledger and visible recovery |
| OPS-09 | 1 / 10 | Install binary with missing or incompatible companion, shader or native library on clean target system | Report actionable startup failure and unwind owned session resources; package declares every required runtime asset and ABI | Clean image package install/run/uninstall logs with intentionally absent dependencies |
| OPS-10 | 1 / 10 | Replay recorded fuzz failure and rerun an old passing case after its security-sensitive dependency changes | Retain minimized seed and invalidate affected old qualification; require evidence matching current source and environment | Seed replay, source/dependency hashes, affected-case mapping and release evidence validation |
