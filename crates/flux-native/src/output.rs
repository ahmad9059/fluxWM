//! Device-free output route planning. A plan is not a reservation, allocation,
//! modeset test or presentation result; all of those require backend transactions.
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub const MAX_CONNECTORS: usize = 64;
pub const MAX_ENCODERS: usize = 128;
pub const MAX_CRTCS: usize = 32;
pub const MAX_PLANES: usize = 128;
pub const MAX_MODES: usize = 256;
pub const MAX_FORMATS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct Format {
    pub code: u32,
    // DRM modifiers are full-width values, including INVALID; never coerce to Linear.
    #[serde(serialize_with = "serialize_u64")]
    pub modifier: u64,
}
fn serialize_u64<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Connection {
    Connected,
    Disconnected,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct Connector {
    pub id: u32,
    pub connection: Connection,
    pub non_desktop: bool,
    pub encoders: Vec<u32>,
    pub current_encoder: Option<u32>,
    /// Original kernel order: selected indices refer to this exact snapshot.
    pub modes: Vec<Mode>,
}
#[derive(Clone, Debug)]
pub struct Encoder {
    pub id: u32,
    pub crtcs: Vec<u32>,
    pub current_crtc: Option<u32>,
}
#[derive(Clone, Debug)]
pub struct Plane {
    pub id: u32,
    pub crtcs: Vec<u32>,
    pub formats: Vec<Format>,
}
#[derive(Clone, Debug, Default)]
pub struct Topology {
    pub connectors: Vec<Connector>,
    pub encoders: Vec<Encoder>,
    pub crtcs: Vec<u32>,
    /// Primary planes only, as classified by the backend.
    pub planes: Vec<Plane>,
    pub render_formats: Vec<Format>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Mode {
    pub width: u16,
    pub height: u16,
    pub hsync_start: u16,
    pub hsync_end: u16,
    pub htotal: u16,
    pub vsync_start: u16,
    pub vsync_end: u16,
    pub vtotal: u16,
    pub clock_khz: u32,
    pub vscan: u16,
    pub interlaced: bool,
    pub double_scan: bool,
    pub preferred: bool,
}
impl Mode {
    /// Rounded rational mHz, without relying on the kernel's integer-Hz label.
    pub fn refresh_millihz(self) -> Option<u32> {
        if self.width == 0
            || self.height == 0
            || self.clock_khz == 0
            || !(self.width <= self.hsync_start
                && self.hsync_start <= self.hsync_end
                && self.hsync_end <= self.htotal)
            || !(self.height <= self.vsync_start
                && self.vsync_start <= self.vsync_end
                && self.vsync_end <= self.vtotal)
        {
            return None;
        }
        let numerator = u64::from(self.clock_khz) * 1_000_000 * if self.interlaced { 2 } else { 1 };
        let denominator = u64::from(self.htotal)
            * u64::from(self.vtotal)
            * u64::from(self.vscan.max(1))
            * if self.double_scan { 2 } else { 1 };
        let refresh = (numerator + denominator / 2) / denominator;
        u32::try_from(refresh).ok().filter(|value| *value > 0)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanError {
    Capacity,
    InvalidTopology,
}
impl std::fmt::Display for PlanError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "output planning failed: {self:?}")
    }
}
impl std::error::Error for PlanError {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Assignment {
    pub connector: u32,
    pub encoder: u32,
    pub crtc: u32,
    pub primary_plane: u32,
    pub mode_index: usize,
    pub mode: Mode,
    pub refresh_millihz: u32,
    /// Exact renderer/plane intersection; allocation and KMS testing still required.
    pub candidate_formats: Vec<Format>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UnassignedReason {
    Disconnected,
    UnknownConnection,
    NonDesktop,
    NoValidMode,
    NoCompatibleRoute,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Unassigned {
    pub connector: u32,
    pub reason: UnassignedReason,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Plan {
    pub assignments: Vec<Assignment>,
    pub unassigned: Vec<Unassigned>,
}
fn unique_ids(ids: impl IntoIterator<Item = u32>, cap: usize) -> Result<BTreeSet<u32>, PlanError> {
    let mut set = BTreeSet::new();
    for id in ids {
        if id == 0 || !set.insert(id) {
            return Err(PlanError::InvalidTopology);
        }
        if set.len() > cap {
            return Err(PlanError::Capacity);
        }
    }
    Ok(set)
}
fn references(ids: &[u32], known: &BTreeSet<u32>) -> Result<(), PlanError> {
    // No valid reference list can exceed the known resource set. Reject before
    // building a set, preserving bounded work and InvalidTopology classification.
    if ids.len() > known.len() {
        return Err(PlanError::InvalidTopology);
    }
    let refs = unique_ids(ids.iter().copied(), known.len())?;
    if !refs.is_subset(known) {
        return Err(PlanError::InvalidTopology);
    }
    Ok(())
}
fn selected_mode(connector: &Connector) -> Option<(usize, Mode, u32)> {
    connector
        .modes
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, mode)| mode.refresh_millihz().map(|refresh| (index, mode, refresh)))
        .max_by_key(|(index, mode, refresh)| {
            (
                mode.preferred,
                u32::from(mode.width) * u32::from(mode.height),
                *refresh,
                std::cmp::Reverse(*index),
            )
        })
}
#[derive(Clone)]
struct Edge {
    to: usize,
    reverse: usize,
    available: bool,
    forward: bool,
}
#[derive(Default)]
struct Graph {
    nodes: Vec<Vec<Edge>>,
}
impl Graph {
    fn node(&mut self) -> usize {
        let id = self.nodes.len();
        self.nodes.push(vec![]);
        id
    }
    fn edge(&mut self, from: usize, to: usize) {
        let reverse = self.nodes[to].len();
        let back = self.nodes[from].len();
        self.nodes[from].push(Edge {
            to,
            reverse,
            available: true,
            forward: true,
        });
        self.nodes[to].push(Edge {
            to: from,
            reverse: back,
            available: false,
            forward: false,
        });
    }
    fn split(&mut self) -> (usize, usize) {
        let a = self.node();
        let b = self.node();
        self.edge(a, b);
        (a, b)
    }
    fn flow(&mut self, source: usize, sink: usize) {
        loop {
            let mut parent = vec![None; self.nodes.len()];
            let mut queue = VecDeque::from([source]);
            parent[source] = Some((source, usize::MAX));
            while let Some(node) = queue.pop_front() {
                for (index, edge) in self.nodes[node].iter().enumerate() {
                    if edge.available && parent[edge.to].is_none() {
                        parent[edge.to] = Some((node, index));
                        queue.push_back(edge.to);
                    }
                }
            }
            if parent[sink].is_none() {
                break;
            }
            let mut node = sink;
            while node != source {
                let (from, index) = parent[node].expect("reachable augmenting path");
                let reverse = self.nodes[from][index].reverse;
                self.nodes[from][index].available = false;
                self.nodes[node][reverse].available = true;
                node = from;
            }
        }
    }
    fn used_to(&self, node: usize) -> Option<usize> {
        self.nodes[node]
            .iter()
            .find(|edge| edge.forward && !edge.available)
            .map(|edge| edge.to)
    }
}
/// Maximum-cardinality independent routes. Stable ID ordering and current-route
/// preference make ties deterministic; this is not minimum-disruption optimization.
pub fn plan(topology: &Topology) -> Result<Plan, PlanError> {
    // Reject before constructing the graph; no partial plan escapes on invalid data.
    if topology.connectors.len() > MAX_CONNECTORS
        || topology.encoders.len() > MAX_ENCODERS
        || topology.crtcs.len() > MAX_CRTCS
        || topology.planes.len() > MAX_PLANES
        || topology.render_formats.len() > MAX_FORMATS
        || topology
            .connectors
            .iter()
            .any(|c| c.modes.len() > MAX_MODES)
        || topology
            .planes
            .iter()
            .any(|p| p.formats.len() > MAX_FORMATS)
    {
        return Err(PlanError::Capacity);
    }
    let connectors = unique_ids(topology.connectors.iter().map(|c| c.id), MAX_CONNECTORS)?;
    let encoders = unique_ids(topology.encoders.iter().map(|e| e.id), MAX_ENCODERS)?;
    let crtcs = unique_ids(topology.crtcs.iter().copied(), MAX_CRTCS)?;
    let planes = unique_ids(topology.planes.iter().map(|p| p.id), MAX_PLANES)?;
    for c in &topology.connectors {
        references(&c.encoders, &encoders)?;
        if c.current_encoder.is_some_and(|id| !encoders.contains(&id)) {
            return Err(PlanError::InvalidTopology);
        }
    }
    for e in &topology.encoders {
        references(&e.crtcs, &crtcs)?;
        if e.current_crtc.is_some_and(|id| !crtcs.contains(&id)) {
            return Err(PlanError::InvalidTopology);
        }
    }
    for p in &topology.planes {
        references(&p.crtcs, &crtcs)?;
    }
    let mut graph = Graph::default();
    let source = graph.node();
    let sink = graph.node();
    let cn: BTreeMap<_, _> = connectors.iter().map(|id| (*id, graph.split())).collect();
    let en: BTreeMap<_, _> = encoders.iter().map(|id| (*id, graph.split())).collect();
    let rn: BTreeMap<_, _> = crtcs.iter().map(|id| (*id, graph.split())).collect();
    let pn: BTreeMap<_, _> = planes.iter().map(|id| (*id, graph.split())).collect();
    let mut result = Plan::default();
    let mut modes = BTreeMap::new();
    let by_connector: BTreeMap<_, _> = topology.connectors.iter().map(|c| (c.id, c)).collect();
    let by_encoder: BTreeMap<_, _> = topology.encoders.iter().map(|e| (e.id, e)).collect();
    let by_plane: BTreeMap<_, _> = topology.planes.iter().map(|p| (p.id, p)).collect();
    for (id, c) in &by_connector {
        let reason = match c.connection {
            Connection::Disconnected => Some(UnassignedReason::Disconnected),
            Connection::Unknown => Some(UnassignedReason::UnknownConnection),
            Connection::Connected if c.non_desktop => Some(UnassignedReason::NonDesktop),
            Connection::Connected => None,
        };
        let mode = selected_mode(c);
        if let Some(reason) =
            reason.or_else(|| mode.is_none().then_some(UnassignedReason::NoValidMode))
        {
            result.unassigned.push(Unassigned {
                connector: *id,
                reason,
            });
            continue;
        }
        modes.insert(*id, mode.expect("checked selected mode"));
        graph.edge(source, cn[id].0);
        let mut candidates = c.encoders.clone();
        candidates.sort_by_key(|id| (Some(*id) != c.current_encoder, *id));
        for encoder in candidates {
            graph.edge(cn[id].1, en[&encoder].0);
        }
    }
    for (id, e) in by_encoder {
        let mut candidates = e.crtcs.clone();
        candidates.sort_by_key(|id| (Some(*id) != e.current_crtc, *id));
        for crtc in candidates {
            graph.edge(en[&id].1, rn[&crtc].0);
        }
    }
    let render: BTreeSet<_> = topology.render_formats.iter().copied().collect();
    let mut formats = BTreeMap::new();
    for (id, p) in by_plane {
        let common: Vec<_> = p
            .formats
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .intersection(&render)
            .copied()
            .collect();
        if common.is_empty() {
            continue;
        }
        formats.insert(id, common);
        for crtc in &crtcs {
            if p.crtcs.contains(crtc) {
                graph.edge(rn[crtc].1, pn[&id].0);
            }
        }
        graph.edge(pn[&id].1, sink);
    }
    graph.flow(source, sink);
    for id in connectors {
        let Some((index, mode, refresh)) = modes.get(&id) else {
            continue;
        };
        let Some(encoder_node) = graph.used_to(cn[&id].1) else {
            result.unassigned.push(Unassigned {
                connector: id,
                reason: UnassignedReason::NoCompatibleRoute,
            });
            continue;
        };
        let encoder = *en
            .iter()
            .find(|(_, nodes)| nodes.0 == encoder_node)
            .expect("encoder node")
            .0;
        let crtc_node = graph.used_to(en[&encoder].1).expect("routed crtc");
        let crtc = *rn
            .iter()
            .find(|(_, nodes)| nodes.0 == crtc_node)
            .expect("crtc node")
            .0;
        let plane_node = graph.used_to(rn[&crtc].1).expect("routed plane");
        let plane = *pn
            .iter()
            .find(|(_, nodes)| nodes.0 == plane_node)
            .expect("plane node")
            .0;
        result.assignments.push(Assignment {
            connector: id,
            encoder,
            crtc,
            primary_plane: plane,
            mode_index: *index,
            mode: *mode,
            refresh_millihz: *refresh,
            candidate_formats: formats[&plane].clone(),
        });
    }
    result.unassigned.sort_by_key(|u| u.connector);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mode() -> Mode {
        Mode {
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
    fn format() -> Format {
        Format {
            code: 1,
            modifier: 0,
        }
    }
    fn graph(mask: u16) -> Topology {
        let links = |shift, id| {
            (0..2)
                .filter(|target| mask & (1u16 << (shift + id * 2 + target)) != 0)
                .map(|target| target + 1)
                .collect()
        };
        Topology {
            connectors: (0..2)
                .map(|id| Connector {
                    id: id + 1,
                    connection: Connection::Connected,
                    non_desktop: false,
                    encoders: links(0, id),
                    current_encoder: None,
                    modes: vec![mode()],
                })
                .collect(),
            encoders: (0..2)
                .map(|id| Encoder {
                    id: id + 1,
                    crtcs: links(4, id),
                    current_crtc: None,
                })
                .collect(),
            crtcs: vec![1, 2],
            planes: (0..2)
                .map(|id| Plane {
                    id: id + 1,
                    crtcs: (0..2)
                        .filter(|crtc| mask & (1u16 << (8 + crtc * 2 + id)) != 0)
                        .map(|crtc| crtc + 1)
                        .collect(),
                    formats: vec![format()],
                })
                .collect(),
            render_formats: vec![format()],
        }
    }
    // Independently enumerate all route combinations, without the flow graph.
    fn oracle(
        topology: &Topology,
        connector: usize,
        encoders: u32,
        crtcs: u32,
        planes: u32,
    ) -> usize {
        if connector == topology.connectors.len() {
            return 0;
        }
        let mut best = oracle(topology, connector + 1, encoders, crtcs, planes);
        for encoder in &topology.encoders {
            if encoders & (1 << encoder.id) != 0
                || !topology.connectors[connector]
                    .encoders
                    .contains(&encoder.id)
            {
                continue;
            }
            for crtc in &encoder.crtcs {
                if crtcs & (1 << crtc) != 0 {
                    continue;
                }
                for plane in &topology.planes {
                    if planes & (1 << plane.id) == 0 && plane.crtcs.contains(crtc) {
                        best = best.max(
                            1 + oracle(
                                topology,
                                connector + 1,
                                encoders | (1 << encoder.id),
                                crtcs | (1 << crtc),
                                planes | (1 << plane.id),
                            ),
                        );
                    }
                }
            }
        }
        best
    }
    fn assert_valid(topology: &Topology, result: &Plan) {
        let mut encoders = BTreeSet::new();
        let mut crtcs = BTreeSet::new();
        let mut planes = BTreeSet::new();
        let mut connectors = BTreeSet::new();
        for assignment in &result.assignments {
            assert!(connectors.insert(assignment.connector));
            assert!(encoders.insert(assignment.encoder));
            assert!(crtcs.insert(assignment.crtc));
            assert!(planes.insert(assignment.primary_plane));
            let connector = topology
                .connectors
                .iter()
                .find(|c| c.id == assignment.connector)
                .unwrap();
            assert!(connector.encoders.contains(&assignment.encoder));
            assert_eq!(connector.modes[assignment.mode_index], assignment.mode);
            assert!(
                topology
                    .encoders
                    .iter()
                    .find(|e| e.id == assignment.encoder)
                    .unwrap()
                    .crtcs
                    .contains(&assignment.crtc)
            );
            let plane = topology
                .planes
                .iter()
                .find(|p| p.id == assignment.primary_plane)
                .unwrap();
            assert!(plane.crtcs.contains(&assignment.crtc));
            assert!(!assignment.candidate_formats.is_empty());
            for format in &assignment.candidate_formats {
                assert!(topology.render_formats.contains(format) && plane.formats.contains(format));
            }
        }
        for unassigned in &result.unassigned {
            assert!(connectors.insert(unassigned.connector));
        }
        assert_eq!(
            connectors,
            topology.connectors.iter().map(|c| c.id).collect()
        );
    }
    #[test]
    fn exhaustive_two_by_two_resource_graphs_match_independent_oracle() {
        for mask in 0..4096 {
            let topology = graph(mask);
            let result = plan(&topology).unwrap();
            assert_eq!(
                result.assignments.len(),
                oracle(&topology, 0, 0, 0, 0),
                "mask={mask}"
            );
            assert_valid(&topology, &result);
        }
    }
    #[test]
    fn reroutes_flexible_output_without_reusing_encoder_crtc_or_plane() {
        let mut topology = graph(4095);
        topology.connectors[1].encoders = vec![1];
        topology.encoders[1].crtcs = vec![2];
        topology.planes[0].crtcs = vec![1];
        topology.planes[1].crtcs = vec![2];
        let result = plan(&topology).unwrap();
        assert_eq!(result.assignments.len(), 2);
        assert_valid(&topology, &result);
        assert_eq!(result.assignments[0].encoder, 2);
        assert_eq!(result.assignments[1].encoder, 1);
        topology.planes.pop();
        assert_eq!(plan(&topology).unwrap().assignments.len(), 1);
        topology.planes.clear();
        assert!(plan(&topology).unwrap().assignments.is_empty());
    }
    #[test]
    fn input_order_is_irrelevant_and_current_routes_break_ties() {
        let mut topology = graph(4095);
        topology.connectors[0].current_encoder = Some(2);
        topology.encoders[1].current_crtc = Some(2);
        let result = plan(&topology).unwrap();
        assert_eq!(result.assignments[0].encoder, 2);
        assert_eq!(result.assignments[0].crtc, 2);
        topology.connectors.reverse();
        topology.encoders.reverse();
        topology.crtcs.reverse();
        topology.planes.reverse();
        for c in &mut topology.connectors {
            c.encoders.reverse();
        }
        for e in &mut topology.encoders {
            e.crtcs.reverse();
        }
        for p in &mut topology.planes {
            p.crtcs.reverse();
        }
        assert_eq!(plan(&topology).unwrap(), result);
    }
    #[test]
    fn excludes_non_desktop_unknown_disconnected_and_invalid_modes() {
        let mut topology = graph(4095);
        for (connection, non_desktop, reason) in [
            (
                Connection::Disconnected,
                false,
                UnassignedReason::Disconnected,
            ),
            (
                Connection::Unknown,
                false,
                UnassignedReason::UnknownConnection,
            ),
            (Connection::Connected, true, UnassignedReason::NonDesktop),
        ] {
            topology.connectors[0].connection = connection;
            topology.connectors[0].non_desktop = non_desktop;
            let result = plan(&topology).unwrap();
            assert_eq!(result.unassigned[0].reason, reason);
            assert_eq!(result.assignments.len(), 1);
        }
        topology.connectors[0].non_desktop = false;
        topology.connectors[0].modes.clear();
        assert_eq!(
            plan(&topology).unwrap().unassigned[0].reason,
            UnassignedReason::NoValidMode
        );
        topology.connectors[0].modes = vec![Mode {
            htotal: 0,
            ..mode()
        }];
        assert_eq!(
            plan(&topology).unwrap().unassigned[0].reason,
            UnassignedReason::NoValidMode
        );
    }
    #[test]
    fn exact_modifier_intersection_is_required_and_serializes_losslessly() {
        let mut topology = graph(4095);
        topology.render_formats = vec![Format {
            modifier: u64::MAX,
            ..format()
        }];
        assert!(plan(&topology).unwrap().assignments.is_empty());
        topology.planes[0].formats = topology.render_formats.clone();
        let result = plan(&topology).unwrap();
        assert_eq!(result.assignments.len(), 1);
        assert!(
            serde_json::to_string(&result)
                .unwrap()
                .contains("\"18446744073709551615\"")
        );
    }
    #[test]
    fn rational_refresh_and_mode_ranking_handle_edge_cases() {
        let base = mode();
        assert_eq!(base.refresh_millihz(), Some(60000));
        assert_eq!(
            Mode {
                interlaced: true,
                ..base
            }
            .refresh_millihz(),
            Some(120000)
        );
        assert_eq!(
            Mode {
                double_scan: true,
                vscan: 2,
                ..base
            }
            .refresh_millihz(),
            Some(15000)
        );
        assert_eq!(
            Mode {
                clock_khz: 148352,
                ..base
            }
            .refresh_millihz(),
            Some(59940)
        );
        for invalid in [
            Mode { width: 0, ..base },
            Mode {
                clock_khz: 0,
                ..base
            },
            Mode {
                hsync_start: 1,
                ..base
            },
            Mode { vtotal: 1, ..base },
            Mode {
                clock_khz: u32::MAX,
                width: 1,
                height: 1,
                hsync_start: 1,
                hsync_end: 1,
                htotal: 1,
                vsync_start: 1,
                vsync_end: 1,
                vtotal: 1,
                ..base
            },
        ] {
            assert!(invalid.refresh_millihz().is_none());
        }
        let mut topology = graph(4095);
        topology.connectors[0].modes = vec![
            Mode {
                preferred: false,
                clock_khz: 297000,
                ..base
            },
            base,
        ];
        assert_eq!(plan(&topology).unwrap().assignments[0].mode_index, 1);
        topology.connectors[0].modes[0].preferred = true;
        assert_eq!(plan(&topology).unwrap().assignments[0].mode_index, 0);
    }
    #[test]
    fn rejects_duplicate_zero_unknown_references_and_capacity_without_partial_plan() {
        let topology = graph(4095);
        let mut bad = topology.clone();
        bad.crtcs.push(1);
        assert_eq!(plan(&bad), Err(PlanError::InvalidTopology));
        bad = topology.clone();
        bad.planes[0].id = 0;
        assert_eq!(plan(&bad), Err(PlanError::InvalidTopology));
        bad = topology.clone();
        bad.connectors[0].encoders.push(3);
        assert!(plan(&bad).is_err());
        bad = topology.clone();
        bad.connectors[0].modes = vec![mode(); MAX_MODES + 1];
        assert_eq!(plan(&bad), Err(PlanError::Capacity));
        bad = topology.clone();
        bad.planes[0].formats = vec![format(); MAX_FORMATS + 1];
        assert_eq!(plan(&bad), Err(PlanError::Capacity));
        assert_eq!(plan(&Topology::default()), Ok(Plan::default()));
        bad = topology.clone();
        bad.encoders.clear();
        assert_eq!(plan(&bad), Err(PlanError::InvalidTopology));
        bad = topology.clone();
        bad.connectors[0].encoders = (1..=10_000).collect();
        assert_eq!(plan(&bad), Err(PlanError::InvalidTopology));
    }
}
