//! Bounded topology acquisition shared by the DRM adapter and fault fixtures.
use super::{Failure, Stage};
use crate::output::{self, Connector, Encoder, Format, Plane, Topology};
use smithay::{
    backend::drm::DrmDevice,
    reexports::drm::control::{self, Device as _, ModeFlags, ModeTypeFlags},
};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Resources {
    connectors: Vec<u32>,
    encoders: Vec<u32>,
    crtcs: Vec<u32>,
}
struct ConnectorRecord<M> {
    connector: Connector,
    modes: Vec<M>,
}
trait Query {
    type Mode;
    fn resources(&mut self) -> Result<Resources, Failure>;
    fn connector(&mut self, id: u32) -> Result<ConnectorRecord<Self::Mode>, Failure>;
    fn encoder(&mut self, id: u32) -> Result<Encoder, Failure>;
    fn primary_planes(&mut self, crtc: u32) -> Result<Vec<(u32, Vec<Format>)>, Failure>;
}
pub(super) struct Inventory<M = control::Mode> {
    pub topology: Topology,
    /// Original kernel modes, indexed by the selected connector/mode_index.
    /// Retained privately; a future commit must revalidate this generation.
    pub modes: BTreeMap<u32, Vec<M>>,
}
fn invalid() -> Failure {
    Failure::at(Stage::OutputTopology)
}
fn io(error: std::io::Error) -> Failure {
    Failure {
        stage: Stage::OutputDiscovery,
        errno: error.raw_os_error(),
    }
}
fn ids(values: &[u32], cap: usize) -> Result<BTreeSet<u32>, Failure> {
    if values.len() > cap {
        return Err(invalid());
    }
    let result: BTreeSet<_> = values.iter().copied().collect();
    if result.len() != values.len() || result.contains(&0) {
        return Err(invalid());
    }
    Ok(result)
}
fn validate(resources: &Resources) -> Result<(), Failure> {
    ids(&resources.connectors, output::MAX_CONNECTORS)?;
    ids(&resources.encoders, output::MAX_ENCODERS)?;
    ids(&resources.crtcs, output::MAX_CRTCS)?;
    Ok(())
}
fn same_resources(a: &Resources, b: &Resources) -> bool {
    let ordered = |values: &[u32]| values.iter().copied().collect::<BTreeSet<_>>();
    ordered(&a.connectors) == ordered(&b.connectors)
        && ordered(&a.encoders) == ordered(&b.encoders)
        && ordered(&a.crtcs) == ordered(&b.crtcs)
}
fn acquire<Q: Query>(
    query: &mut Q,
    render_formats: Vec<Format>,
) -> Result<Inventory<Q::Mode>, Failure> {
    let resources = query.resources()?;
    validate(&resources)?;
    if render_formats.len() > output::MAX_FORMATS {
        return Err(invalid());
    }
    let mut topology = Topology {
        crtcs: resources.crtcs.clone(),
        render_formats,
        ..Topology::default()
    };
    let mut modes = BTreeMap::new();
    for id in &resources.connectors {
        let record = query.connector(*id)?;
        if record.connector.id != *id
            || record.connector.modes.len() != record.modes.len()
            || record.modes.len() > output::MAX_MODES
        {
            return Err(invalid());
        }
        modes.insert(*id, record.modes);
        topology.connectors.push(record.connector);
    }
    for id in &resources.encoders {
        let encoder = query.encoder(*id)?;
        if encoder.id != *id {
            return Err(invalid());
        }
        topology.encoders.push(encoder);
    }
    let mut planes: BTreeMap<u32, Plane> = BTreeMap::new();
    for crtc in &resources.crtcs {
        let records = query.primary_planes(*crtc)?;
        if records.len() > output::MAX_PLANES {
            return Err(invalid());
        }
        let mut seen = BTreeSet::new();
        for (id, mut formats) in records {
            if id == 0 || !seen.insert(id) || formats.len() > output::MAX_FORMATS {
                return Err(invalid());
            }
            formats.sort();
            formats.dedup();
            if let Some(existing) = planes.get_mut(&id) {
                if existing.formats != formats {
                    return Err(invalid());
                }
                existing.crtcs.push(*crtc);
            } else {
                if planes.len() == output::MAX_PLANES {
                    return Err(invalid());
                }
                planes.insert(
                    id,
                    Plane {
                        id,
                        crtcs: vec![*crtc],
                        formats,
                    },
                );
            }
        }
    }
    topology.planes = planes.into_values().collect();
    let after = query.resources()?;
    validate(&after)?;
    if !same_resources(&resources, &after) {
        return Err(invalid());
    }
    // Validate every reference even for disconnected/non-desktop connectors.
    output::plan(&topology).map_err(|_| invalid())?;
    Ok(Inventory { topology, modes })
}
struct DrmQuery<'a> {
    drm: &'a DrmDevice,
    resources: control::ResourceHandles,
    first: bool,
}
impl Query for DrmQuery<'_> {
    type Mode = control::Mode;
    fn resources(&mut self) -> Result<Resources, Failure> {
        let resources = if self.first {
            self.first = false;
            self.resources.clone()
        } else {
            self.drm.resource_handles().map_err(io)?
        };
        Ok(Resources {
            connectors: resources
                .connectors()
                .iter()
                .map(|id| u32::from(*id))
                .collect(),
            encoders: resources
                .encoders()
                .iter()
                .map(|id| u32::from(*id))
                .collect(),
            crtcs: resources.crtcs().iter().map(|id| u32::from(*id)).collect(),
        })
    }
    fn connector(&mut self, id: u32) -> Result<ConnectorRecord<Self::Mode>, Failure> {
        let handle = control::from_u32(id).ok_or_else(invalid)?;
        // Startup, explicit device change and resume are the only callers.
        let info = self.drm.get_connector(handle, true).map_err(io)?;
        let properties = self.drm.get_properties(handle).map_err(io)?;
        if properties.as_props_and_values().0.len() > 256 {
            return Err(invalid());
        }
        let mut non_desktop = false;
        for (property, value) in properties {
            let info = self.drm.get_property(property).map_err(io)?;
            if info.name().to_bytes() == b"non-desktop" {
                non_desktop = info
                    .value_type()
                    .convert_value(value)
                    .as_boolean()
                    .ok_or_else(invalid)?;
            }
        }
        if info.modes().len() > output::MAX_MODES || info.encoders().len() > output::MAX_ENCODERS {
            return Err(invalid());
        }
        let modes = info.modes().to_vec();
        let connection = match info.state() {
            control::connector::State::Connected => output::Connection::Connected,
            control::connector::State::Disconnected => output::Connection::Disconnected,
            control::connector::State::Unknown => output::Connection::Unknown,
        };
        Ok(ConnectorRecord {
            connector: Connector {
                id,
                connection,
                non_desktop,
                encoders: info.encoders().iter().map(|id| u32::from(*id)).collect(),
                current_encoder: info.current_encoder().map(u32::from),
                modes: modes
                    .iter()
                    .map(|mode| {
                        let (width, height) = mode.size();
                        let (hsync_start, hsync_end, htotal) = mode.hsync();
                        let (vsync_start, vsync_end, vtotal) = mode.vsync();
                        output::Mode {
                            width,
                            height,
                            hsync_start,
                            hsync_end,
                            htotal,
                            vsync_start,
                            vsync_end,
                            vtotal,
                            clock_khz: mode.clock(),
                            vscan: mode.vscan(),
                            interlaced: mode.flags().contains(ModeFlags::INTERLACE),
                            double_scan: mode.flags().contains(ModeFlags::DBLSCAN),
                            preferred: mode.mode_type().contains(ModeTypeFlags::PREFERRED),
                        }
                    })
                    .collect(),
            },
            modes,
        })
    }
    fn encoder(&mut self, id: u32) -> Result<Encoder, Failure> {
        let info = self
            .drm
            .get_encoder(control::from_u32(id).ok_or_else(invalid)?)
            .map_err(io)?;
        Ok(Encoder {
            id,
            crtcs: self
                .resources
                .filter_crtcs(info.possible_crtcs())
                .into_iter()
                .map(u32::from)
                .collect(),
            current_crtc: info.crtc().map(u32::from),
        })
    }
    fn primary_planes(&mut self, crtc: u32) -> Result<Vec<(u32, Vec<Format>)>, Failure> {
        self.drm
            .planes(&control::from_u32(crtc).ok_or_else(invalid)?)
            .map_err(|_| Failure::at(Stage::OutputDiscovery))?
            .primary
            .into_iter()
            .map(|plane| {
                let formats: Vec<_> = plane
                    .formats
                    .iter()
                    .take(output::MAX_FORMATS + 1)
                    .map(|format| Format {
                        code: format.code as u32,
                        modifier: format.modifier.into(),
                    })
                    .collect();
                Ok((plane.handle.into(), formats))
            })
            .collect()
    }
}
pub(super) struct PreparedOutputs<M> {
    pub plan: Option<output::Plan>,
    pub failure: Option<Failure>,
    pub modes: BTreeMap<u32, Vec<M>>,
}
fn finish<M>(inventory: Result<Inventory<M>, Failure>) -> PreparedOutputs<M> {
    match inventory.and_then(|inventory| {
        let plan = output::plan(&inventory.topology).map_err(|_| invalid())?;
        Ok((plan, inventory.modes))
    }) {
        Ok((plan, modes)) => PreparedOutputs {
            plan: Some(plan),
            failure: None,
            modes,
        },
        Err(failure) => PreparedOutputs {
            plan: None,
            failure: Some(failure),
            modes: BTreeMap::new(),
        },
    }
}
pub(super) fn prepare(
    drm: &DrmDevice,
    render_formats: Vec<Format>,
) -> PreparedOutputs<control::Mode> {
    finish(discover(drm, render_formats))
}
pub(super) fn discover(drm: &DrmDevice, render_formats: Vec<Format>) -> Result<Inventory, Failure> {
    let resources = drm.resource_handles().map_err(io)?;
    acquire(
        &mut DrmQuery {
            drm,
            resources,
            first: true,
        },
        render_formats,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Fixture {
        resources: Resources,
        after: Resources,
        calls: usize,
        fail_at: Option<usize>,
        inconsistent_plane: bool,
        bad_mode_count: bool,
    }
    fn fixture() -> Fixture {
        let resources = Resources {
            connectors: vec![1, 2],
            encoders: vec![3, 4],
            crtcs: vec![5, 6],
        };
        Fixture {
            after: resources.clone(),
            resources,
            calls: 0,
            fail_at: None,
            inconsistent_plane: false,
            bad_mode_count: false,
        }
    }
    fn format() -> Format {
        Format {
            code: 1,
            modifier: 0,
        }
    }
    fn mode() -> output::Mode {
        output::Mode {
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
        }
    }
    impl Fixture {
        fn step(&mut self) -> Result<(), Failure> {
            self.calls += 1;
            if Some(self.calls) == self.fail_at {
                Err(Failure {
                    stage: Stage::OutputDiscovery,
                    errno: Some(libc::EIO),
                })
            } else {
                Ok(())
            }
        }
    }
    impl Query for Fixture {
        type Mode = usize;
        fn resources(&mut self) -> Result<Resources, Failure> {
            self.step()?;
            Ok(if self.calls == 1 {
                self.resources.clone()
            } else {
                self.after.clone()
            })
        }
        fn connector(&mut self, id: u32) -> Result<ConnectorRecord<usize>, Failure> {
            self.step()?;
            Ok(ConnectorRecord {
                connector: Connector {
                    id,
                    connection: output::Connection::Connected,
                    non_desktop: false,
                    encoders: vec![3, 4],
                    current_encoder: None,
                    modes: vec![mode()],
                },
                modes: if self.bad_mode_count {
                    vec![]
                } else {
                    vec![id as usize * 100]
                },
            })
        }
        fn encoder(&mut self, id: u32) -> Result<Encoder, Failure> {
            self.step()?;
            Ok(Encoder {
                id,
                crtcs: vec![5, 6],
                current_crtc: None,
            })
        }
        fn primary_planes(&mut self, crtc: u32) -> Result<Vec<(u32, Vec<Format>)>, Failure> {
            self.step()?;
            Ok(vec![(
                7,
                vec![if self.inconsistent_plane && crtc == 6 {
                    Format {
                        modifier: 1,
                        ..format()
                    }
                } else {
                    format()
                }],
            )])
        }
    }
    #[test]
    fn format_pair_capacity_failure_is_an_explicit_unavailable_plan() {
        let mut query = fixture();
        let prepared = finish(acquire(
            &mut query,
            (0..=output::MAX_FORMATS)
                .map(|modifier| Format {
                    modifier: modifier as u64,
                    ..format()
                })
                .collect(),
        ));
        assert!(prepared.plan.is_none());
        assert_eq!(prepared.failure, Some(invalid()));
        assert!(prepared.modes.is_empty());
        assert_eq!(query.calls, 1);
        let prepared = finish(acquire(&mut fixture(), vec![format()]));
        assert!(prepared.plan.is_some());
        assert!(prepared.failure.is_none());
        assert_eq!(prepared.modes.len(), 2);
    }
    #[test]
    fn acquisition_retains_original_modes_and_shared_plane_has_one_owner() {
        let mut query = fixture();
        let Inventory { topology, modes } = acquire(&mut query, vec![format()]).unwrap();
        assert_eq!(query.calls, 8);
        assert_eq!(modes[&1], [100]);
        assert_eq!(modes[&2], [200]);
        assert_eq!(topology.planes.len(), 1);
        assert_eq!(topology.planes[0].crtcs, [5, 6]);
        let result = output::plan(&topology).unwrap();
        assert_eq!(result.assignments.len(), 1);
        assert_eq!(
            modes[&result.assignments[0].connector][result.assignments[0].mode_index],
            100
        );
    }
    #[test]
    fn every_query_failure_aborts_without_publishing_partial_inventory() {
        for fail_at in 1..=8 {
            let mut query = fixture();
            query.fail_at = Some(fail_at);
            let failure = acquire(&mut query, vec![format()]).err().unwrap();
            assert_eq!(
                failure,
                Failure {
                    stage: Stage::OutputDiscovery,
                    errno: Some(libc::EIO)
                }
            );
            assert_eq!(query.calls, fail_at);
        }
    }
    #[test]
    fn detects_resource_churn_but_accepts_resource_reordering() {
        let mut query = fixture();
        query.after.connectors.pop();
        assert_eq!(acquire(&mut query, vec![format()]).err(), Some(invalid()));
        query = fixture();
        query.after.connectors.reverse();
        query.after.encoders.reverse();
        query.after.crtcs.reverse();
        assert!(acquire(&mut query, vec![format()]).is_ok());
    }
    #[test]
    fn rejects_over_capacity_resources_before_object_queries() {
        let mut query = fixture();
        query.resources.connectors = vec![1; output::MAX_CONNECTORS + 1];
        assert_eq!(acquire(&mut query, vec![format()]).err(), Some(invalid()));
        assert_eq!(query.calls, 1);
        query = fixture();
        query.resources.crtcs.push(5);
        assert_eq!(acquire(&mut query, vec![format()]).err(), Some(invalid()));
        assert_eq!(query.calls, 1);
    }
    #[test]
    fn rejects_inconsistent_plane_formats_and_mode_index_records() {
        let mut query = fixture();
        query.inconsistent_plane = true;
        assert_eq!(acquire(&mut query, vec![format()]).err(), Some(invalid()));
        query = fixture();
        query.bad_mode_count = true;
        assert_eq!(acquire(&mut query, vec![format()]).err(), Some(invalid()));
        assert_eq!(query.calls, 2);
    }
}
