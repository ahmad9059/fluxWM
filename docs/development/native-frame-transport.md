# Native frame transport and ownership

The optional `flux-native/system` adapter reads DRM events through an owned, nonblocking calloop source. It preserves the full 64-bit kernel cookie; the pinned higher-level notifier loses that identity. GPU preparation requires `CRTCInVBlankEvent`. Reader failure disables delivery and marks transport unhealthy. No physical session has been executed for this change.

The checked decoder validates an entire bounded 4096-byte batch before delivering completions. Malformed lengths, truncated events, absent CRTC identity and invalid microseconds fail closed. Unknown event kinds are skipped. Zero-cookie flips remain observable but cannot complete an owned frame. Sequence numbers and timestamps do not identify ownership.

The atomic request serializer bounds object/property arrays and preserves a 64-bit cookie. The later [live submission adapter](native-live-submit.md) adds owned-cookie NONBLOCK/PAGE_FLIP_EVENT commits and blocking rollback. The diagnostic entry point remains fixed `TEST_ONLY | ALLOW_MODESET`, cookie zero. `python3 tools/check_drm_abi.py --help` describes the device-free C/Rust size, offset, ioctl and flag comparison against locally installed Linux headers.

## Frame ledger contract

[The frame ledger](../../crates/flux-native/src/frames.rs) owns prepared resources and their scenes. One persistent pool admits at most 64 slots, 256 MiB nominal payload and 32 output ledgers. These are accounting bounds, not measured GPU-memory or RSS limits. Rendering and acquire synchronization must complete before preparation returns.

An accepted submission retains pending and previous front resources. Only the exact pending cookie, GPU generation/session epoch and CRTC can advance the front; the previous front is then retired. Definite rejection returns the prepared frame. Uncertain submission, timeout, invalidation and backend panic retain consumer ownership. Scene visibility stops on invalidation or uncertainty. Sequence wrap and reused CRTC identities do not authorize completion.

A conclusive backend quiescence proof permits final retirement. Constructing that proof is an unsafe platform obligation; pause, hotplug, timeout and a reused FD number are insufficient. Unexpected ledger destruction deliberately retains consuming payloads and their quota permits. This is failure containment, not a production shutdown strategy. Creating replacement pools would bypass that accounting; the future backend must keep one pool for its lifetime.

## Remaining implementation

The [live submission adapter](native-live-submit.md) now joins the ledger and real reader to synchronized RGB targets and atomic commits; its hardware path remains unexecuted. The main compositor still accepts only the nested backend. Native work still needs compositor scene rendering/publication, multi-output transactions, complete output/session recovery and physical qualification. Cookies are unique only within one process; restart must establish a fresh or conclusively quiesced DRM event stream before live submissions. No hardware, cross-process restart or physical presentation claim follows from these tests.

Focused checks cover malformed batches, full-width cookies, sequence wrap, owned socket dispatch, admission/rejection, stale epochs, uncertainty, panic retention and nominal quota behavior. Physical DRM/GBM/GLES submission, driver failures and session transitions remain unexecuted. See [production gaps](production-status-2026-10-18.md) for the wider release requirements.
