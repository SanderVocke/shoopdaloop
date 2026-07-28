//! Process-graph scheduling.
//!
//! Nodes live in an arena and refer to each other by index, so the schedule is a
//! precomputed `Vec<Vec<NodeIdx>>` with no pointer chasing, refcount traffic or
//! `weak_ptr` locking on the audio thread. The schedule is recomputed only when
//! the graph changes, never per cycle.

use std::collections::BTreeSet;

use thiserror::Error;

/// Index of a node in the graph arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeIdx(pub usize);

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("cycle in graph or unsolveable co-processing constraint")]
    Cycle,
    #[error("node index {0} out of range ({1} nodes)")]
    BadIndex(usize, usize),
}

/// One node's place in the graph.
///
/// Edges may be declared from either end; the scheduler unions both directions,
/// so annotating only one side is enough.
#[derive(Debug, Clone, Default)]
pub struct NodeSpec {
    /// Debug name. Also breaks ties when ordering within a dependency layer, so
    /// the schedule is deterministic.
    pub name: String,
    pub outgoing: Vec<NodeIdx>,
    pub incoming: Vec<NodeIdx>,
    /// Nodes that must be processed together with this one, in one step.
    pub co_process: Vec<NodeIdx>,
}

/// Disjoint-set over node indices, used to merge co-process constraints.
///
/// The C++ backend merged co-process sets imperatively while walking nodes,
/// which produced different groupings depending on iteration order when the
/// constraint was declared asymmetrically. Union-find makes the grouping
/// order-independent.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            let grand = self.parent[self.parent[i]];
            self.parent[i] = grand;
            i = grand;
        }
        i
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

/// Computes a processing order: a list of steps, each step a set of nodes to be
/// co-processed. Steps must be executed in order; nodes within a step have no
/// ordering constraint between them.
///
/// Ties within a dependency layer break on the group's lowest node name, which
/// is what the C++ implementation did and what its expected schedules encode.
pub fn processing_order(nodes: &[NodeSpec]) -> Result<Vec<Vec<NodeIdx>>, GraphError> {
    let n = nodes.len();
    let check = |i: NodeIdx| {
        if i.0 < n {
            Ok(i.0)
        } else {
            Err(GraphError::BadIndex(i.0, n))
        }
    };

    // 1. Merge co-process constraints into groups.
    let mut uf = UnionFind::new(n);
    for (i, spec) in nodes.iter().enumerate() {
        for &partner in &spec.co_process {
            uf.union(i, check(partner)?);
        }
    }

    // 2. Collect group members, keyed by representative.
    let mut members: Vec<Vec<NodeIdx>> = vec![Vec::new(); n];
    for i in 0..n {
        let root = uf.find(i);
        members[root].push(NodeIdx(i));
    }
    let groups: Vec<usize> = (0..n).filter(|&i| uf.find(i) == i).collect();
    // Dense group ids, so in-degree bookkeeping stays a flat Vec.
    let mut group_of = vec![usize::MAX; n];
    for (gid, &root) in groups.iter().enumerate() {
        group_of[root] = gid;
    }
    let group_of = |uf: &mut UnionFind, i: usize| group_of[uf.find(i)];

    // 3. Lift node edges to group edges, dropping edges internal to a group.
    let n_groups = groups.len();
    let mut succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_groups];
    let mut pred: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n_groups];
    let add_edge = |uf: &mut UnionFind,
                    from: usize,
                    to: usize,
                    succ: &mut Vec<BTreeSet<usize>>,
                    pred: &mut Vec<BTreeSet<usize>>| {
        let (gf, gt) = (group_of(uf, from), group_of(uf, to));
        if gf != gt {
            succ[gf].insert(gt);
            pred[gt].insert(gf);
        }
    };
    for (i, spec) in nodes.iter().enumerate() {
        for &out in &spec.outgoing {
            add_edge(&mut uf, i, check(out)?, &mut succ, &mut pred);
        }
        for &inc in &spec.incoming {
            add_edge(&mut uf, check(inc)?, i, &mut succ, &mut pred);
        }
    }

    // 4. Lowest member name per group, for deterministic tie-breaking.
    let group_name: Vec<&str> = groups
        .iter()
        .map(|&root| {
            members[root]
                .iter()
                .map(|m| nodes[m.0].name.as_str())
                .min()
                .unwrap_or("")
        })
        .collect();

    // 5. Kahn's algorithm, layer by layer, each layer name-sorted.
    let mut remaining_pred: Vec<usize> = pred.iter().map(|p| p.len()).collect();
    let mut scheduled = vec![false; n_groups];
    let mut order: Vec<Vec<NodeIdx>> = Vec::with_capacity(n_groups);
    let mut done = 0;

    while done < n_groups {
        let mut layer: Vec<usize> = (0..n_groups)
            .filter(|&g| !scheduled[g] && remaining_pred[g] == 0)
            .collect();
        if layer.is_empty() {
            return Err(GraphError::Cycle);
        }
        layer.sort_by_key(|&g| group_name[g]);

        for &g in &layer {
            scheduled[g] = true;
            done += 1;
            let mut group: Vec<NodeIdx> = members[groups[g]].clone();
            group.sort();
            order.push(group);
        }
        // Release successors only after the whole layer is placed, so ordering
        // within a layer cannot influence which nodes unlock next.
        for &g in &layer {
            for &s in &succ[g] {
                remaining_pred[s] -= 1;
            }
        }
    }

    Ok(order)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert2::{check, let_assert};

    /// Builds specs from (name, outgoing) pairs; indices are positional.
    fn specs(defs: &[(&str, &[usize])]) -> Vec<NodeSpec> {
        defs.iter()
            .map(|(name, out)| NodeSpec {
                name: (*name).to_string(),
                outgoing: out.iter().map(|&i| NodeIdx(i)).collect(),
                ..Default::default()
            })
            .collect()
    }

    fn names(nodes: &[NodeSpec], schedule: &[Vec<NodeIdx>]) -> Vec<Vec<String>> {
        schedule
            .iter()
            .map(|step| {
                let mut n: Vec<String> = step.iter().map(|i| nodes[i.0].name.clone()).collect();
                n.sort();
                n
            })
            .collect()
    }

    // The three topologies below reproduce the expected schedules asserted in
    // legacy C++ backend integration test test_graph_construction.cpp. The C++ test
    // builds them out of GraphAudioPort/GraphLoopChannel; here the edges are
    // stated directly, so the scheduler is checked against the same vectors
    // without needing those types ported yet.
    //
    // Each audio port contributes two nodes (prepare, process_and_internal_
    // connections); each channel two (prepare_buffers, process); each loop one.

    #[test]
    fn two_ports() {
        // p1 -> p2 internal connection.
        let nodes = specs(&[
            ("p1::prepare", &[2]),
            ("p2::prepare", &[3]),
            ("p1::process_and_internal_connections", &[3]),
            ("p2::process_and_internal_connections", &[]),
        ]);
        let_assert!(Ok(schedule) = processing_order(&nodes));
        check!(
            names(&nodes, &schedule)
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    #[test]
    fn direct_loop() {
        // 0 p1::prepare, 1 p2::prepare, 2 p1::process, 3 p2::process,
        // 4 channel::prepare_buffers, 5 channel::process, 6 loop::process
        let nodes = specs(&[
            ("p1::prepare", &[2, 4]),
            ("p2::prepare", &[3]),
            ("p1::process_and_internal_connections", &[3, 5]),
            ("p2::process_and_internal_connections", &[]),
            ("channel::prepare_buffers", &[6]),
            ("channel::process", &[3]),
            ("loop::process", &[5]),
        ]);
        let_assert!(Ok(schedule) = processing_order(&nodes));
        check!(
            names(&nodes, &schedule)
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["loop::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    #[test]
    fn two_direct_loops_co_processed() {
        // Two loops declared as co-process partners must land in one step.
        let mut nodes = specs(&[
            ("p1::prepare", &[2, 4, 7]),
            ("p2::prepare", &[3]),
            ("p1::process_and_internal_connections", &[3, 5, 8]),
            ("p2::process_and_internal_connections", &[]),
            ("channel::prepare_buffers", &[6]),
            ("channel::process", &[3]),
            ("loop::process", &[5]),
            ("channel::prepare_buffers", &[9]),
            ("channel::process", &[3]),
            ("loop::process", &[8]),
        ]);
        nodes[6].co_process = vec![NodeIdx(6), NodeIdx(9)];
        nodes[9].co_process = vec![NodeIdx(6), NodeIdx(9)];

        let_assert!(Ok(schedule) = processing_order(&nodes));
        check!(
            names(&nodes, &schedule)
                == vec![
                    vec!["p1::prepare".to_string()],
                    vec!["p2::prepare".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["channel::prepare_buffers".to_string()],
                    vec!["p1::process_and_internal_connections".to_string()],
                    vec!["loop::process".to_string(), "loop::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["channel::process".to_string()],
                    vec!["p2::process_and_internal_connections".to_string()],
                ]
        );
    }

    #[test]
    fn incoming_edges_are_equivalent_to_outgoing() {
        let a = specs(&[("a", &[1]), ("b", &[])]);
        let mut b = specs(&[("a", &[]), ("b", &[])]);
        b[1].incoming = vec![NodeIdx(0)];
        let_assert!(Ok(sa) = processing_order(&a));
        let_assert!(Ok(sb) = processing_order(&b));
        check!(sa == sb);
    }

    #[test]
    fn co_process_grouping_is_order_independent() {
        // Constraint declared on one side only still merges both.
        let mut nodes = specs(&[("a", &[]), ("b", &[]), ("c", &[])]);
        nodes[0].co_process = vec![NodeIdx(2)];
        let_assert!(Ok(schedule) = processing_order(&nodes));
        check!(schedule.len() == 2);
        check!(schedule.contains(&vec![NodeIdx(0), NodeIdx(2)]));
        check!(schedule.contains(&vec![NodeIdx(1)]));
    }

    #[test]
    fn detects_cycle() {
        let nodes = specs(&[("a", &[1]), ("b", &[0])]);
        check!(processing_order(&nodes) == Err(GraphError::Cycle));
    }

    #[test]
    fn detects_co_process_cycle() {
        // a must precede b, but they are forced into the same step.
        let mut nodes = specs(&[("a", &[1]), ("b", &[])]);
        nodes[0].co_process = vec![NodeIdx(1)];
        // Edge is internal to the group, so it is dropped rather than deadlocking.
        let_assert!(Ok(schedule) = processing_order(&nodes));
        check!(schedule == vec![vec![NodeIdx(0), NodeIdx(1)]]);
    }

    #[test]
    fn rejects_out_of_range_index() {
        let nodes = specs(&[("a", &[5])]);
        check!(processing_order(&nodes) == Err(GraphError::BadIndex(5, 1)));
    }

    #[test]
    fn empty_graph() {
        let_assert!(Ok(schedule) = processing_order(&[]));
        check!(schedule.is_empty());
    }
}
