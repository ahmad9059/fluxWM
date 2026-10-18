# Author disposition: native target diagnostic follow-up

Claude Opus 5.5 independently inspected frozen `c2e86ed`. The [unedited report](claude-report.md), [execution](execution.json), [manifest](input-manifest.json) and [verified trace](verified-execution.json) retain its actual Read/Grep-only scope. It made no edits and executed no tests, hardware or kernel calls.

The follow-up finds **no reachable high or medium issue within the inspected scope** and accepts both earlier medium corrections. It traces target/local-resource destruction before final cache cleanup, verifies the pinned renderer uses weak DMA-BUF keys for both import/binding caches, and confirms cleanup errors cannot leave a successful report. It traces cloned connector removal, disabled source CRTCs, plane moves/swaps and explicit conflicts when an unrelated surviving output would lose its primary plane. The actual adapter consumes the reconciled routing and retains fixed TEST_ONLY flags.

The remaining lows are explicit limitations, not silently waived requirements:

- Properties not set by the hypothetical request remain inherited: connector bpc/HDR/colorspace/range, CRTC LUT/CTM/VRR and plane blend/color/z-position/filter state can change test feasibility. Neither pixel output nor complete SDR/black display policy is qualified.
- The report lists selected targets but not every hypothetical connector detachment, disabled CRTC or cleared plane. It must not be read as a complete display-change preview. Unrelated outputs remain in the hypothetical state.
- The reconciler conservatively counts disconnected or non-desktop routed connectors as surviving users; a conflict can be a false negative. There is no alternate route search.
- Query work is finite but can be large: on the order of tens of thousands of property/ioctl calls in worst-case admitted snapshots, with no universal deadline.
- The encoder is validated but cannot be forced through the connector-to-CRTC atomic property request; the driver may choose another compatible encoder.

Additional fixture coverage is still needed for multi-output CRTC/plane swaps, several source CRTCs being disabled, already inactive sources and selected planes whose source is also tested. The request builder/property acquisition and real GLES cache cleanup are not executed by existing fixtures. A constant flag test is not a kernel execution test. None of this qualifies the native adapter, hardware, presentation, frame lifetimes, safe preview or a full phase gate.

No runtime source changes occurred after this frozen follow-up; later author work retains evidence and expands these limitations in documentation. This is static review plus separate author device-free evidence, not production certification.
