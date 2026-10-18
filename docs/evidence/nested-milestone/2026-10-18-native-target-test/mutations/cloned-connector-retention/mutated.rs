//! Reconcile the hypothetical test against existing connector/plane routing.
use super::{TestFailure, TestStage};
use crate::output::{Assignment, MAX_CONNECTORS, MAX_CRTCS};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Clone, Copy, Debug)]
pub struct Route {
    pub id: u32,
    pub crtc: Option<u32>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct Reconciled {
    /// All known connector routes, including unchanged routes. This prevents a
    /// tested CRTC from retaining a previously cloned/unassigned connector.
    pub connectors: BTreeMap<u32, Option<u32>>,
    pub disabled_crtcs: BTreeSet<u32>,
    pub cleared_planes: BTreeSet<u32>,
}
fn invalid() -> TestFailure {
    TestFailure::at(TestStage::StaleTopology)
}
fn map(routes: &[Route], known: &BTreeSet<u32>) -> Result<BTreeMap<u32, Option<u32>>, TestFailure> {
    let mut result = BTreeMap::new();
    for route in routes {
        if route.id == 0
            || route.crtc.is_some_and(|id| !known.contains(&id))
            || result.insert(route.id, route.crtc).is_some()
        {
            return Err(invalid());
        }
    }
    Ok(result)
}
pub fn reconcile(
    assignments: &[Assignment],
    connectors: &[Route],
    planes: &[Route],
    crtcs: &[u32],
) -> Result<Reconciled, TestFailure> {
    if assignments.len() > super::MAX_TEST_TARGETS
        || connectors.len() > MAX_CONNECTORS
        || planes.len() > 256
        || crtcs.len() > MAX_CRTCS
    {
        return Err(TestFailure::at(TestStage::Budget));
    }
    let known: BTreeSet<_> = crtcs.iter().copied().collect();
    if known.len() != crtcs.len() || known.contains(&0) {
        return Err(invalid());
    }
    let before = map(connectors, &known)?;
    let plane_routes = map(planes, &known)?;
    let mut after = before.clone();
    let mut tested = BTreeSet::new();
    let mut selected = BTreeSet::new();
    let mut planned = BTreeSet::new();
    let mut sources = BTreeSet::new();
    for a in assignments {
        if !before.contains_key(&a.connector)
            || !plane_routes.contains_key(&a.primary_plane)
            || !known.contains(&a.crtc)
            || !tested.insert(a.crtc)
            || !selected.insert(a.primary_plane)
            || !planned.insert(a.connector)
        {
            return Err(invalid());
        }
        if let Some(source) = before[&a.connector] {
            sources.insert(source);
        }
        if let Some(source) = plane_routes[&a.primary_plane] {
            sources.insert(source);
        }
        after.insert(a.connector, Some(a.crtc));
    }
    for (connector, crtc) in &mut after {
        if false && !planned.contains(connector) && crtc.is_some_and(|id| tested.contains(&id)) {
            *crtc = None;
        }
    }
    let mut disabled = BTreeSet::new();
    for source in sources.difference(&tested) {
        let remaining = after.values().any(|crtc| *crtc == Some(*source));
        let primary_stolen = assignments
            .iter()
            .any(|a| plane_routes[&a.primary_plane] == Some(*source));
        if remaining && primary_stolen {
            return Err(TestFailure::at(TestStage::TopologyConflict));
        }
        if !remaining {
            disabled.insert(*source);
        }
    }
    let affected: BTreeSet<_> = tested.union(&disabled).copied().collect();
    let cleared = plane_routes
        .iter()
        .filter(|(id, crtc)| {
            !selected.contains(id) && crtc.is_some_and(|id| affected.contains(&id))
        })
        .map(|(id, _)| *id)
        .collect();
    Ok(Reconciled {
        connectors: after,
        disabled_crtcs: disabled,
        cleared_planes: cleared,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    fn assignment(connector: u32, crtc: u32, plane: u32) -> Assignment {
        Assignment {
            connector,
            crtc,
            primary_plane: plane,
            encoder: 1,
            mode_index: 0,
            mode: crate::output::Mode {
                width: 1,
                height: 1,
                hsync_start: 1,
                hsync_end: 1,
                htotal: 1,
                vsync_start: 1,
                vsync_end: 1,
                vtotal: 1,
                clock_khz: 1,
                vscan: 1,
                interlaced: false,
                double_scan: false,
                preferred: true,
            },
            refresh_millihz: 1000000,
            candidate_formats: vec![],
        }
    }
    #[test]
    fn detaches_unplanned_clones_and_preserves_unrelated_routes() {
        let result = reconcile(
            &[assignment(2, 10, 20)],
            &[
                Route {
                    id: 1,
                    crtc: Some(10),
                },
                Route { id: 2, crtc: None },
                Route {
                    id: 3,
                    crtc: Some(11),
                },
            ],
            &[
                Route {
                    id: 20,
                    crtc: Some(10),
                },
                Route {
                    id: 21,
                    crtc: Some(10),
                },
                Route {
                    id: 22,
                    crtc: Some(11),
                },
            ],
            &[10, 11],
        )
        .unwrap();
        assert_eq!(result.connectors[&1], None);
        assert_eq!(result.connectors[&2], Some(10));
        assert_eq!(result.connectors[&3], Some(11));
        assert_eq!(result.cleared_planes, BTreeSet::from([21]));
        assert!(result.disabled_crtcs.is_empty());
    }
    #[test]
    fn disables_source_crtc_losing_last_connector_and_clears_its_planes() {
        let result = reconcile(
            &[assignment(1, 11, 20)],
            &[Route {
                id: 1,
                crtc: Some(10),
            }],
            &[
                Route {
                    id: 20,
                    crtc: Some(10),
                },
                Route {
                    id: 21,
                    crtc: Some(10),
                },
            ],
            &[10, 11],
        )
        .unwrap();
        assert_eq!(result.disabled_crtcs, BTreeSet::from([10]));
        assert_eq!(result.cleared_planes, BTreeSet::from([21]));
        assert_eq!(result.connectors[&1], Some(11));
    }
    #[test]
    fn rejects_stealing_primary_from_unrelated_surviving_output() {
        let result = reconcile(
            &[assignment(1, 11, 20)],
            &[
                Route { id: 1, crtc: None },
                Route {
                    id: 2,
                    crtc: Some(10),
                },
            ],
            &[Route {
                id: 20,
                crtc: Some(10),
            }],
            &[10, 11],
        );
        assert_eq!(result.unwrap_err().stage, TestStage::TopologyConflict);
    }
    #[test]
    fn rejects_unknown_duplicate_and_over_capacity_snapshot_before_planning() {
        let a = assignment(1, 10, 20);
        let connectors = [Route { id: 1, crtc: None }];
        let planes = [Route { id: 20, crtc: None }];
        assert!(reconcile(std::slice::from_ref(&a), &connectors, &planes, &[10]).is_ok());
        assert!(reconcile(std::slice::from_ref(&a), &connectors, &planes, &[10, 10]).is_err());
        assert!(
            reconcile(
                std::slice::from_ref(&a),
                &[connectors[0], connectors[0]],
                &planes,
                &[10]
            )
            .is_err()
        );
        assert!(reconcile(std::slice::from_ref(&a), &connectors, &[], &[10]).is_err());
        assert!(
            reconcile(
                &[a],
                &vec![connectors[0]; MAX_CONNECTORS + 1],
                &planes,
                &[10]
            )
            .is_err()
        );
    }
}
