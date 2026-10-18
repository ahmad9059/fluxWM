//! Bounded render-target allocation/test coordinator. It never requests a live
//! commit; the platform adapter is responsible for an atomic TEST_ONLY request.
pub mod routes;
use crate::output::{Assignment, Format, Plan};
use serde::Serialize;

pub const MAX_TEST_TARGETS: usize = 32;
pub const MAX_TEST_PIXELS: u64 = 33_554_432;
pub const MAX_TEST_DIMENSION: u16 = 16_384;
pub const MAX_TEST_ATTEMPTS: usize = 64;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum TestStage {
    Inactive,
    UnsupportedLegacy,
    MissingPlan,
    NoTargets,
    Budget,
    StaleTopology,
    TopologyConflict,
    Properties,
    Allocate,
    AllocatedFormat,
    Export,
    Bind,
    Render,
    Sync,
    ModeBlob,
    AtomicTest,
    Cleanup,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct TestFailure {
    pub stage: TestStage,
    pub errno: Option<i32>,
}
impl TestFailure {
    pub fn at(stage: TestStage) -> Self {
        Self { stage, errno: None }
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct TestedTarget {
    pub connector: u32,
    pub crtc: u32,
    pub primary_plane: u32,
    pub format: Format,
}
#[derive(Clone, Debug, Serialize)]
pub struct TestReport {
    pub ok: bool,
    pub planned_targets: usize,
    pub unassigned_connectors: usize,
    pub attempts: usize,
    /// Nominal width*height*4, not actual padded/compressed driver memory usage.
    pub nominal_bytes: String,
    pub tested_targets: Vec<TestedTarget>,
    pub failure: Option<TestFailure>,
    pub cleanup_failures: String,
}
/// Targets cannot escape run(); all successful and failed partial preparation
/// must retire before its report is returned. Platform allocations may block.
pub trait Backend {
    type Target;
    fn atomic(&self) -> bool;
    fn prepare(
        &mut self,
        assignment: &Assignment,
        format: Format,
    ) -> Result<Self::Target, TestFailure>;
    fn test_only(&mut self, targets: &[Self::Target]) -> Result<(), TestFailure>;
    fn cleanup_failures(&self) -> u64;
    /// Runs after every owned target has dropped, including on partial failure.
    fn finish_cleanup(&mut self) -> Result<(), TestFailure> {
        Ok(())
    }
}
fn formats(assignment: &Assignment) -> Vec<Format> {
    // Known four-byte RGB render targets. Policy does not alias ARGB/XRGB or
    // implicit/Linear modifiers. Other formats need a separate render contract.
    const XRGB: u32 = u32::from_le_bytes(*b"XR24");
    const ARGB: u32 = u32::from_le_bytes(*b"AR24");
    let mut formats: Vec<_> = assignment
        .candidate_formats
        .iter()
        .copied()
        .filter(|f| f.code == XRGB || f.code == ARGB)
        .collect();
    formats.sort_by_key(|f| (f.code != XRGB, f.modifier));
    formats.dedup();
    formats
}
fn admission(plan: &Plan) -> Result<u64, TestFailure> {
    if plan.assignments.is_empty() {
        return Err(TestFailure::at(TestStage::NoTargets));
    }
    if plan.assignments.len() > MAX_TEST_TARGETS {
        return Err(TestFailure::at(TestStage::Budget));
    }
    let mut pixels = 0u64;
    let mut connectors = std::collections::BTreeSet::new();
    let mut encoders = std::collections::BTreeSet::new();
    let mut crtcs = std::collections::BTreeSet::new();
    let mut planes = std::collections::BTreeSet::new();
    for a in &plan.assignments {
        if a.mode.width == 0
            || a.mode.height == 0
            || a.mode.width > MAX_TEST_DIMENSION
            || a.mode.height > MAX_TEST_DIMENSION
            || a.mode.refresh_millihz().is_none()
            || a.connector == 0
            || a.encoder == 0
            || a.crtc == 0
            || a.primary_plane == 0
            || !connectors.insert(a.connector)
            || !encoders.insert(a.encoder)
            || !crtcs.insert(a.crtc)
            || !planes.insert(a.primary_plane)
            || a.candidate_formats.len() > crate::output::MAX_FORMATS
        {
            return Err(TestFailure::at(TestStage::Budget));
        }
        pixels = pixels
            .checked_add(u64::from(a.mode.width) * u64::from(a.mode.height))
            .ok_or_else(|| TestFailure::at(TestStage::Budget))?;
        if pixels > MAX_TEST_PIXELS {
            return Err(TestFailure::at(TestStage::Budget));
        }
    }
    Ok(pixels * 4)
}
pub fn run<B: Backend>(plan: &Plan, backend: &mut B) -> TestReport {
    let mut report = TestReport {
        ok: false,
        planned_targets: plan.assignments.len(),
        unassigned_connectors: plan.unassigned.len(),
        attempts: 0,
        nominal_bytes: "0".into(),
        tested_targets: vec![],
        failure: None,
        cleanup_failures: "0".into(),
    };
    let result = (|| {
        if !backend.atomic() {
            return Err(TestFailure::at(TestStage::UnsupportedLegacy));
        }
        report.nominal_bytes = admission(plan)?.to_string();
        let mut targets = vec![];
        for assignment in &plan.assignments {
            let mut prepared = None;
            let mut last = TestFailure::at(TestStage::AllocatedFormat);
            for format in formats(assignment) {
                if backend.cleanup_failures() > 0 {
                    return Err(TestFailure::at(TestStage::Cleanup));
                }
                if report.attempts == MAX_TEST_ATTEMPTS {
                    return Err(TestFailure::at(TestStage::Budget));
                }
                report.attempts += 1;
                match backend.prepare(assignment, format) {
                    Ok(target) => {
                        prepared = Some(target);
                        report.tested_targets.push(TestedTarget {
                            connector: assignment.connector,
                            crtc: assignment.crtc,
                            primary_plane: assignment.primary_plane,
                            format,
                        });
                        break;
                    }
                    Err(error)
                        if matches!(
                            error.stage,
                            TestStage::Allocate
                                | TestStage::AllocatedFormat
                                | TestStage::Export
                                | TestStage::Bind
                                | TestStage::Render
                        ) =>
                    {
                        last = error
                    }
                    Err(error) => return Err(error),
                }
            }
            targets.push(prepared.ok_or(last)?);
        }
        // Every target and mode/blob remains alive through the one joint test.
        backend.test_only(&targets)?;
        Ok(())
        // targets retire before checking cleanup accounting below, on all paths.
    })();
    report.failure = result.err();
    let final_cleanup = if false && report.attempts > 0 {
        backend.finish_cleanup()
    } else {
        Ok(())
    };
    let failures = backend
        .cleanup_failures()
        .saturating_add(u64::from(final_cleanup.is_err()));
    if report.failure.is_none() {
        report.failure = final_cleanup.err();
    }
    report.cleanup_failures = failures.to_string();
    if failures > 0 && report.failure.is_none() {
        report.failure = Some(TestFailure::at(TestStage::Cleanup));
    }
    report.ok = report.failure.is_none();
    if !report.ok {
        report.tested_targets.clear();
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::Mode;
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    const XRGB: u32 = u32::from_le_bytes(*b"XR24");
    const ARGB: u32 = u32::from_le_bytes(*b"AR24");
    fn assignment(id: u32) -> Assignment {
        Assignment {
            connector: id,
            encoder: id,
            crtc: id,
            primary_plane: id,
            mode_index: 0,
            mode: Mode {
                width: 1920,
                height: 1080,
                hsync_start: 2008,
                hsync_end: 2052,
                htotal: 2200,
                vsync_start: 1084,
                vsync_end: 1089,
                vtotal: 1125,
                clock_khz: 148500,
                vscan: 1,
                interlaced: false,
                double_scan: false,
                preferred: true,
            },
            refresh_millihz: 60000,
            candidate_formats: vec![
                Format {
                    code: XRGB,
                    modifier: 0,
                },
                Format {
                    code: ARGB,
                    modifier: 0,
                },
            ],
        }
    }
    fn plan(count: u32) -> Plan {
        Plan {
            assignments: (1..=count).map(assignment).collect(),
            unassigned: vec![],
        }
    }
    #[derive(Default)]
    struct Shared {
        live: Cell<usize>,
        failures: Cell<u64>,
        log: RefCell<Vec<&'static str>>,
        fail_cleanup: Cell<bool>,
        cached: Cell<usize>,
    }
    struct Target(Rc<Shared>);
    impl Drop for Target {
        fn drop(&mut self) {
            self.0.live.set(self.0.live.get() - 1);
            self.0.log.borrow_mut().push("drop");
            if self.0.fail_cleanup.get() {
                self.0.failures.set(self.0.failures.get() + 1);
            }
        }
    }
    #[derive(Default)]
    struct Mock {
        shared: Rc<Shared>,
        legacy: bool,
        failures: Vec<TestFailure>,
        formats: Vec<Format>,
        test_failure: bool,
        cache_failure: bool,
    }
    impl Backend for Mock {
        type Target = Target;
        fn atomic(&self) -> bool {
            !self.legacy
        }
        fn cleanup_failures(&self) -> u64 {
            self.shared.failures.get()
        }
        fn prepare(&mut self, _: &Assignment, format: Format) -> Result<Target, TestFailure> {
            self.shared.log.borrow_mut().push("prepare");
            self.formats.push(format);
            if let Some(error) = self.failures.get(self.formats.len() - 1) {
                return Err(*error);
            }
            self.shared.live.set(self.shared.live.get() + 1);
            self.shared.cached.set(self.shared.cached.get() + 1);
            Ok(Target(self.shared.clone()))
        }
        fn finish_cleanup(&mut self) -> Result<(), TestFailure> {
            assert_eq!(
                self.shared.live.get(),
                0,
                "cache cleanup precedes target retirement"
            );
            self.shared.log.borrow_mut().push("cache-cleanup");
            if self.cache_failure {
                return Err(TestFailure::at(TestStage::Cleanup));
            }
            self.shared.cached.set(0);
            Ok(())
        }
        fn test_only(&mut self, targets: &[Target]) -> Result<(), TestFailure> {
            assert_eq!(self.shared.live.get(), targets.len());
            assert!(!targets.is_empty());
            self.shared.log.borrow_mut().push("test");
            if self.test_failure {
                Err(TestFailure {
                    stage: TestStage::AtomicTest,
                    errno: Some(libc::EINVAL),
                })
            } else {
                Ok(())
            }
        }
    }
    #[test]
    fn all_targets_live_during_joint_test_and_retire_before_return() {
        let mut backend = Mock::default();
        let report = run(&plan(2), &mut backend);
        assert!(report.ok);
        assert_eq!(report.tested_targets.len(), 2);
        assert_eq!(report.nominal_bytes, (2 * 1920 * 1080 * 4u64).to_string());
        assert_eq!(
            *backend.shared.log.borrow(),
            [
                "prepare",
                "prepare",
                "test",
                "drop",
                "drop",
                "cache-cleanup"
            ]
        );
        assert_eq!(backend.shared.live.get(), 0);
    }
    #[test]
    fn legacy_empty_and_budget_reject_before_allocating() {
        let mut backend = Mock {
            legacy: true,
            ..Default::default()
        };
        assert_eq!(
            run(&plan(1), &mut backend).failure.unwrap().stage,
            TestStage::UnsupportedLegacy
        );
        assert!(backend.formats.is_empty());
        backend.legacy = false;
        assert_eq!(
            run(&plan(0), &mut backend).failure.unwrap().stage,
            TestStage::NoTargets
        );
        for mut invalid in [plan(33), plan(2), plan(1), plan(1)] {
            if invalid.assignments.len() == 2 {
                invalid.assignments[1].crtc = 1;
            }
            if invalid.assignments.len() == 1 {
                invalid.assignments[0].mode.width = u16::MAX;
            }
            assert_eq!(
                run(&invalid, &mut backend).failure.unwrap().stage,
                TestStage::Budget
            );
        }
        assert!(backend.formats.is_empty());
        let mut too_large = plan(32); // 32 full-HD targets exceed total nominal pixels.
        assert_eq!(
            run(&too_large, &mut backend).failure.unwrap().stage,
            TestStage::Budget
        );
        too_large.assignments.truncate(1);
        assert!(run(&too_large, &mut backend).ok);
    }
    #[test]
    fn format_fallback_preserves_exact_pairs_and_xrgb_priority() {
        let mut p = plan(1);
        p.assignments[0].candidate_formats = vec![
            Format {
                code: ARGB,
                modifier: 0,
            },
            Format {
                code: XRGB,
                modifier: u64::MAX,
            },
            Format {
                code: XRGB,
                modifier: 0,
            },
            Format {
                code: 999,
                modifier: 0,
            },
        ];
        let mut backend = Mock {
            failures: vec![TestFailure::at(TestStage::Allocate)],
            ..Default::default()
        };
        let report = run(&p, &mut backend);
        assert!(report.ok);
        assert_eq!(report.attempts, 2);
        assert_eq!(
            backend.formats,
            [
                Format {
                    code: XRGB,
                    modifier: 0
                },
                Format {
                    code: XRGB,
                    modifier: u64::MAX
                }
            ]
        );
        assert_eq!(report.tested_targets[0].format, backend.formats[1]);
    }
    #[test]
    fn fatal_prepare_failure_drops_prior_target_and_never_tests() {
        let mut backend = Mock {
            failures: vec![TestFailure::at(TestStage::Sync)],
            ..Default::default()
        };
        assert_eq!(
            run(&plan(1), &mut backend).failure.unwrap().stage,
            TestStage::Sync
        );
        assert_eq!(backend.formats.len(), 1);
        struct SecondFails(Mock);
        impl Backend for SecondFails {
            type Target = Target;
            fn atomic(&self) -> bool {
                true
            }
            fn cleanup_failures(&self) -> u64 {
                self.0.cleanup_failures()
            }
            fn prepare(&mut self, a: &Assignment, f: Format) -> Result<Target, TestFailure> {
                if a.connector == 2 {
                    Err(TestFailure::at(TestStage::StaleTopology))
                } else {
                    self.0.prepare(a, f)
                }
            }
            fn finish_cleanup(&mut self) -> Result<(), TestFailure> {
                self.0.finish_cleanup()
            }
            fn test_only(&mut self, _: &[Target]) -> Result<(), TestFailure> {
                panic!("partial batch cannot be tested")
            }
        }
        let mut backend = SecondFails(Mock::default());
        let report = run(&plan(2), &mut backend);
        assert_eq!(report.failure.unwrap().stage, TestStage::StaleTopology);
        assert!(report.tested_targets.is_empty());
        assert_eq!(backend.0.shared.live.get(), 0);
        assert_eq!(
            *backend.0.shared.log.borrow(),
            ["prepare", "drop", "cache-cleanup"]
        );
    }
    #[test]
    fn failed_joint_test_preserves_error_and_cleans_every_target() {
        let mut backend = Mock {
            test_failure: true,
            ..Default::default()
        };
        backend.shared.fail_cleanup.set(true);
        let report = run(&plan(2), &mut backend);
        assert!(!report.ok);
        assert_eq!(
            report.failure.unwrap(),
            TestFailure {
                stage: TestStage::AtomicTest,
                errno: Some(libc::EINVAL)
            }
        );
        assert_eq!(report.cleanup_failures, "2");
        assert!(report.tested_targets.is_empty());
        assert_eq!(backend.shared.live.get(), 0);
    }
    #[test]
    fn cleanup_failure_cannot_be_reported_as_a_successful_test() {
        let mut backend = Mock::default();
        backend.shared.fail_cleanup.set(true);
        let report = run(&plan(1), &mut backend);
        assert!(!report.ok);
        assert_eq!(report.failure.unwrap().stage, TestStage::Cleanup);
        assert_eq!(report.cleanup_failures, "1");
        assert!(report.tested_targets.is_empty());
    }
    #[test]
    fn renderer_cache_retires_after_targets_and_errors_cannot_pass() {
        let mut backend = Mock::default();
        let report = run(&plan(1), &mut backend);
        assert!(report.ok);
        assert_eq!(backend.shared.cached.get(), 0);
        let mut backend = Mock {
            cache_failure: true,
            ..Default::default()
        };
        let report = run(&plan(1), &mut backend);
        assert!(!report.ok);
        assert_eq!(report.failure.unwrap().stage, TestStage::Cleanup);
        assert_eq!(report.cleanup_failures, "1");
        assert!(report.tested_targets.is_empty());
        assert_eq!(backend.shared.live.get(), 0);
    }
    #[test]
    fn allocation_retries_have_a_global_attempt_budget() {
        let mut p = plan(1);
        p.assignments[0].candidate_formats = (0..256)
            .map(|modifier| Format {
                code: XRGB,
                modifier,
            })
            .collect();
        let mut backend = Mock {
            failures: vec![TestFailure::at(TestStage::Allocate); 256],
            ..Default::default()
        };
        let report = run(&p, &mut backend);
        assert_eq!(report.failure.unwrap().stage, TestStage::Budget);
        assert_eq!(report.attempts, MAX_TEST_ATTEMPTS);
        assert_eq!(backend.formats.len(), MAX_TEST_ATTEMPTS);
        assert!(!backend.shared.log.borrow().contains(&"test"));
    }
}
