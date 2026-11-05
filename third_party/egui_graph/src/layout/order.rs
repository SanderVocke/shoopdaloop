//! Crossing minimisation over a proper layered graph.
//!
//! Implements the port-rank barycenter heuristic of Schulze, Spönemann and
//! von Hanxleden, "Drawing Layered Graphs with Port Constraints" (JVLC 2014):
//! barycenters and crossing counts are computed per socket connection point
//! ("port") rather than per node centre, making the ordering socket-aware.

use super::{anchor, CGraph};

/// A proper layered graph: every segment spans exactly one layer gap.
///
/// Vertices `0..num_real` are the component's nodes; the rest are dummy
/// vertices introduced to split edges spanning multiple layers.
pub(super) struct ProperGraph {
    /// The number of real vertices.
    pub(super) num_real: usize,
    /// The vertex order within each layer.
    pub(super) layers: Vec<Vec<usize>>,
    /// The in-layer position of every vertex (inverse of `layers`).
    pub(super) pos: Vec<usize>,
    /// All edge segments.
    pub(super) segments: Vec<Segment>,
    /// Per-vertex segments connecting to the previous layer.
    pub(super) back: Vec<Vec<usize>>,
    /// Per-vertex segments connecting to the next layer.
    pub(super) fwd: Vec<Vec<usize>>,
    /// Per input edge, the chain of dummy vertices created for it, ordered
    /// from the earlier to the later layer.
    pub(super) edge_dummies: Vec<Vec<usize>>,
}

/// A piece of an edge spanning one layer gap, from `src` to `dst` in the
/// following layer.
pub(super) struct Segment {
    pub(super) src: usize,
    pub(super) dst: usize,
    /// Cross-axis offset of the connection point from `src`'s centre.
    pub(super) src_anchor: f32,
    /// Cross-axis offset of the connection point from `dst`'s centre.
    pub(super) dst_anchor: f32,
    /// The rank of the connection point among `src`'s forward ports.
    pub(super) src_port: PortRank,
    /// The rank of the connection point among `dst`'s backward ports.
    pub(super) dst_port: PortRank,
    /// The number of segments in the edge that this segment is a piece of.
    pub(super) chain: usize,
}

/// A port's position among the distinct connection points on one side of a
/// vertex, ordered along the cross axis.
#[derive(Clone, Copy)]
pub(super) struct PortRank {
    pub(super) index: usize,
    pub(super) count: usize,
}

/// Identifies a distinct connection point on a real vertex.
///
/// Multi-edges sharing a socket share a port; an input and an output socket
/// are distinct ports even when their offsets coincide.
#[derive(Clone, Copy)]
struct PortKey {
    anchor: f32,
    output: bool,
    socket: usize,
}

impl PortRank {
    /// Fractional in-`(0, 1)` cross-order offset used in barycenters.
    fn frac(self) -> f32 {
        (self.index + 1) as f32 / (self.count + 1) as f32
    }
}

/// Build the proper layered graph for a component, splitting edges that span
/// multiple layers with dummy vertices and resolving the port rank of every
/// segment endpoint.
///
/// The initial vertex ordering is a DFS from the sources in index order.
pub(super) fn build_proper(cg: &CGraph, reversed: &[bool], layer: &[usize]) -> ProperGraph {
    let num_real = cg.size_main.len();
    let num_layers = layer.iter().map(|&l| l + 1).max().unwrap_or(0);
    let mut layer_of = layer.to_vec();
    let mut segments = Vec::new();
    let mut edge_dummies = Vec::with_capacity(cg.edges.len());
    // The socket-derived key of each segment endpoint, for resolving port
    // ranks once all segments exist. `None` for dummy endpoints.
    let mut endpoint_keys: Vec<(Option<PortKey>, Option<PortKey>)> = Vec::new();

    for (e, edge) in cg.edges.iter().enumerate() {
        let out_key = PortKey {
            anchor: anchor(&cg.out_anchors[edge.src], edge.src_socket),
            output: true,
            socket: edge.src_socket,
        };
        let in_key = PortKey {
            anchor: anchor(&cg.in_anchors[edge.dst], edge.dst_socket),
            output: false,
            socket: edge.dst_socket,
        };
        // Orient the segment chain from the earlier to the later layer;
        // reversed edges keep their true socket anchors.
        let ((a, a_key), (b, b_key)) = if reversed[e] {
            ((edge.dst, in_key), (edge.src, out_key))
        } else {
            ((edge.src, out_key), (edge.dst, in_key))
        };
        debug_assert!(layer_of[a] < layer_of[b]);
        let chain = layer_of[b] - layer_of[a];
        let mut dummies = Vec::new();
        let mut prev = (a, Some(a_key));
        for l in layer_of[a] + 1..layer_of[b] {
            let dummy = layer_of.len();
            layer_of.push(l);
            dummies.push(dummy);
            push_segment(
                &mut segments,
                &mut endpoint_keys,
                chain,
                prev,
                (dummy, None),
            );
            prev = (dummy, None);
        }
        push_segment(
            &mut segments,
            &mut endpoint_keys,
            chain,
            prev,
            (b, Some(b_key)),
        );
        edge_dummies.push(dummies);
    }

    // Resolve port ranks from the distinct keys per vertex and direction.
    let mut fwd_keys: Vec<Vec<PortKey>> = vec![Vec::new(); num_real];
    let mut back_keys: Vec<Vec<PortKey>> = vec![Vec::new(); num_real];
    for (seg, &(src_key, dst_key)) in segments.iter().zip(&endpoint_keys) {
        if let Some(key) = src_key {
            fwd_keys[seg.src].push(key);
        }
        if let Some(key) = dst_key {
            back_keys[seg.dst].push(key);
        }
    }
    for keys in fwd_keys.iter_mut().chain(back_keys.iter_mut()) {
        keys.sort_by(key_cmp);
        keys.dedup_by(|a, b| key_cmp(a, b).is_eq());
    }
    for (seg, &(src_key, dst_key)) in segments.iter_mut().zip(&endpoint_keys) {
        if let Some(key) = src_key {
            seg.src_port = port_rank(&fwd_keys[seg.src], key);
        }
        if let Some(key) = dst_key {
            seg.dst_port = port_rank(&back_keys[seg.dst], key);
        }
    }

    let num_v = layer_of.len();
    let mut fwd: Vec<Vec<usize>> = vec![Vec::new(); num_v];
    let mut back: Vec<Vec<usize>> = vec![Vec::new(); num_v];
    for (s, seg) in segments.iter().enumerate() {
        fwd[seg.src].push(s);
        back[seg.dst].push(s);
    }

    // Initial ordering: DFS from the sources in index order, appending each
    // vertex to its layer on first visit.
    let mut layers: Vec<Vec<usize>> = vec![Vec::new(); num_layers];
    let mut visited = vec![false; num_v];
    let mut stack: Vec<usize> = (0..num_v).rev().filter(|&v| back[v].is_empty()).collect();
    while let Some(v) = stack.pop() {
        if visited[v] {
            continue;
        }
        visited[v] = true;
        layers[layer_of[v]].push(v);
        for &s in fwd[v].iter().rev() {
            stack.push(segments[s].dst);
        }
    }
    // Every vertex of a weakly-connected DAG is reachable from its sources,
    // but stay total in case of an unexpected input.
    for v in 0..num_v {
        if !visited[v] {
            layers[layer_of[v]].push(v);
        }
    }

    let mut pos = vec![0usize; num_v];
    for layer in &layers {
        for (i, &v) in layer.iter().enumerate() {
            pos[v] = i;
        }
    }

    ProperGraph {
        num_real,
        layers,
        pos,
        segments,
        back,
        fwd,
        edge_dummies,
    }
}

/// Reorder vertices within their layers to reduce segment crossings via
/// alternating port-rank barycenter sweeps, keeping the best ordering
/// encountered.
pub(super) fn minimize_crossings(g: &mut ProperGraph) {
    /// Sweep pairs rarely improve the ordering beyond this bound.
    const MAX_SWEEP_PAIRS: usize = 4;
    let mut best = g.layers.clone();
    let mut best_crossings = count_crossings(g);
    for _ in 0..MAX_SWEEP_PAIRS {
        if best_crossings == 0 {
            break;
        }
        for l in 1..g.layers.len() {
            reorder_layer(g, l, true);
        }
        for l in (0..g.layers.len().saturating_sub(1)).rev() {
            reorder_layer(g, l, false);
        }
        let crossings = count_crossings(g);
        if crossings < best_crossings {
            best_crossings = crossings;
            best.clone_from(&g.layers);
        } else {
            break;
        }
    }
    g.layers = best;
    for layer in &g.layers {
        for (i, &v) in layer.iter().enumerate() {
            g.pos[v] = i;
        }
    }
}

fn push_segment(
    segments: &mut Vec<Segment>,
    endpoint_keys: &mut Vec<(Option<PortKey>, Option<PortKey>)>,
    chain: usize,
    (src, src_key): (usize, Option<PortKey>),
    (dst, dst_key): (usize, Option<PortKey>),
) {
    segments.push(Segment {
        src,
        dst,
        src_anchor: src_key.map_or(0.0, |key| key.anchor),
        dst_anchor: dst_key.map_or(0.0, |key| key.anchor),
        src_port: PortRank { index: 0, count: 1 },
        dst_port: PortRank { index: 0, count: 1 },
        chain,
    });
    endpoint_keys.push((src_key, dst_key));
}

/// Order port keys along the cross axis.
fn key_cmp(a: &PortKey, b: &PortKey) -> std::cmp::Ordering {
    a.anchor
        .total_cmp(&b.anchor)
        .then(a.output.cmp(&b.output))
        .then(a.socket.cmp(&b.socket))
}

/// The rank of `key` within a vertex side's sorted, deduplicated port list.
fn port_rank(keys: &[PortKey], key: PortKey) -> PortRank {
    let index = keys
        .iter()
        .position(|k| key_cmp(k, &key).is_eq())
        .unwrap_or(0);
    PortRank {
        index,
        count: keys.len(),
    }
}

/// Stable-sort the vertices of layer `l` by the mean port rank of their
/// neighbours on the fixed side; vertices without neighbours on that side
/// keep their position.
fn reorder_layer(g: &mut ProperGraph, l: usize, toward_back: bool) {
    let mut keyed: Vec<(f32, usize)> = g.layers[l]
        .iter()
        .map(|&v| {
            let segs = if toward_back { &g.back[v] } else { &g.fwd[v] };
            let key = if segs.is_empty() {
                g.pos[v] as f32
            } else {
                let sum: f32 = segs
                    .iter()
                    .map(|&s| {
                        let seg = &g.segments[s];
                        if toward_back {
                            g.pos[seg.src] as f32 + seg.src_port.frac()
                        } else {
                            g.pos[seg.dst] as f32 + seg.dst_port.frac()
                        }
                    })
                    .sum();
                sum / segs.len() as f32
            };
            (key, v)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
    for (i, &(_, v)) in keyed.iter().enumerate() {
        g.pos[v] = i;
        g.layers[l][i] = v;
    }
}

fn count_crossings(g: &ProperGraph) -> usize {
    (0..g.layers.len().saturating_sub(1))
        .map(|l| count_crossings_bilayer(g, l))
        .sum()
}

/// Count crossings between the segments spanning the gap after layer `l`.
///
/// Counted at port level: segments are keyed by `(vertex position, port
/// index)` at both ends, and a pair crosses iff its keys are strictly
/// oppositely ordered. Segments sharing a port never cross each other.
fn count_crossings_bilayer(g: &ProperGraph, l: usize) -> usize {
    let keys: Vec<((usize, usize), (usize, usize))> = g.layers[l]
        .iter()
        .flat_map(|&v| {
            g.fwd[v].iter().map(|&s| {
                let seg = &g.segments[s];
                (
                    (g.pos[seg.src], seg.src_port.index),
                    (g.pos[seg.dst], seg.dst_port.index),
                )
            })
        })
        .collect();
    let mut crossings = 0;
    for (i, &(a_src, a_dst)) in keys.iter().enumerate() {
        for &(b_src, b_dst) in &keys[i + 1..] {
            if (a_src < b_src && a_dst > b_dst) || (a_src > b_src && a_dst < b_dst) {
                crossings += 1;
            }
        }
    }
    crossings
}

#[cfg(test)]
mod tests {
    use super::super::{CEdge, CGraph};
    use super::*;

    /// A graph of `num` nodes with sockets anchored at node centres.
    fn cgraph(num: usize, edges: &[(usize, usize, usize, usize)]) -> CGraph {
        CGraph {
            size_main: vec![10.0; num],
            size_cross: vec![10.0; num],
            in_anchors: vec![Vec::new(); num],
            out_anchors: vec![Vec::new(); num],
            edges: edges
                .iter()
                .map(|&(src, src_socket, dst, dst_socket)| CEdge {
                    src,
                    src_socket,
                    dst,
                    dst_socket,
                })
                .collect(),
        }
    }

    fn proper(cg: &CGraph) -> ProperGraph {
        let pairs: Vec<_> = cg.edges.iter().map(|e| (e.src, e.dst)).collect();
        let reversed = vec![false; pairs.len()];
        let layer = super::super::rank::assign_layers(cg.size_main.len(), &pairs);
        build_proper(cg, &reversed, &layer)
    }

    #[test]
    fn long_edges_split_with_dummies() {
        let cg = cgraph(3, &[(0, 0, 1, 0), (1, 0, 2, 0), (0, 0, 2, 0)]);
        let g = proper(&cg);
        assert_eq!(g.num_real, 3);
        // The two-layer edge gains one dummy vertex and splits in two.
        assert_eq!(g.pos.len(), 4);
        assert_eq!(g.segments.len(), 4);
        // The dummy chain is recorded against its edge.
        assert_eq!(g.edge_dummies, vec![vec![], vec![], vec![3]]);
    }

    #[test]
    fn barycenter_uncrosses_node_order() {
        // `0` fans to `2` and `3`; `1` connects only to `2`, so `2` should
        // end up beside `1`.
        let cg = cgraph(4, &[(0, 0, 3, 0), (0, 0, 2, 0), (1, 0, 2, 0)]);
        let mut g = proper(&cg);
        minimize_crossings(&mut g);
        assert_eq!(count_crossings(&g), 0);
    }

    #[test]
    fn port_ranks_disambiguate_socket_order() {
        // `0`'s sockets fan to `1` and `2` in reverse order: socket 1 (lower)
        // feeds `1`, socket 0 (upper) feeds `2`. Node-centre barycenters tie,
        // but port ranks order the layer as [2, 1].
        let mut cg = cgraph(3, &[(0, 1, 1, 0), (0, 0, 2, 0)]);
        cg.out_anchors[0] = vec![-10.0, 10.0];
        let mut g = proper(&cg);
        assert_eq!(count_crossings(&g), 1);
        minimize_crossings(&mut g);
        assert_eq!(count_crossings(&g), 0);
        assert_eq!(g.layers[1], vec![2, 1]);
    }

    #[test]
    fn crossed_parallel_sockets_counted() {
        // Two edges between the same node pair, with crossed socket indices.
        let mut crossed = cgraph(2, &[(0, 0, 1, 1), (0, 1, 1, 0)]);
        crossed.out_anchors[0] = vec![-5.0, 5.0];
        crossed.in_anchors[1] = vec![-5.0, 5.0];
        assert_eq!(count_crossings(&proper(&crossed)), 1);

        let mut straight = cgraph(2, &[(0, 0, 1, 0), (0, 1, 1, 1)]);
        straight.out_anchors[0] = vec![-5.0, 5.0];
        straight.in_anchors[1] = vec![-5.0, 5.0];
        assert_eq!(count_crossings(&proper(&straight)), 0);
    }

    #[test]
    fn sweeps_never_worsen_crossings() {
        let cg = cgraph(
            6,
            &[
                (0, 0, 3, 0),
                (0, 0, 4, 0),
                (1, 0, 5, 0),
                (1, 0, 3, 0),
                (2, 0, 4, 0),
                (2, 0, 5, 0),
            ],
        );
        let mut g = proper(&cg);
        let before = count_crossings(&g);
        minimize_crossings(&mut g);
        assert!(count_crossings(&g) <= before);
    }
}
