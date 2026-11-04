//! Greedy cycle breaking.

/// For each edge, whether it must be traversed in reverse for the graph to be
/// acyclic.
///
/// Implements the greedy heuristic of Eades, Lin and Smyth: repeatedly remove
/// sinks (prepending to the right of a vertex sequence), then sources
/// (appending to the left), then - if any vertices remain - the vertex
/// maximising `outdeg - indeg`. Edges pointing right-to-left in the resulting
/// sequence are reversed.
///
/// Self-loops must be filtered out beforehand.
pub(super) fn break_cycles(num_nodes: usize, edges: &[(usize, usize)]) -> Vec<bool> {
    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
    let mut in_adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
    for &(src, dst) in edges {
        debug_assert!(src != dst, "self-loops must be filtered out");
        out_adj[src].push(dst);
        in_adj[dst].push(src);
    }
    let mut outdeg: Vec<usize> = out_adj.iter().map(Vec::len).collect();
    let mut indeg: Vec<usize> = in_adj.iter().map(Vec::len).collect();

    let mut alive = vec![true; num_nodes];
    let mut remaining = num_nodes;
    let mut left = Vec::with_capacity(num_nodes);
    let mut right = Vec::new();
    let remove = |v: usize, alive: &mut [bool], indeg: &mut [usize], outdeg: &mut [usize]| {
        alive[v] = false;
        for &w in &out_adj[v] {
            if alive[w] {
                indeg[w] -= 1;
            }
        }
        for &w in &in_adj[v] {
            if alive[w] {
                outdeg[w] -= 1;
            }
        }
    };
    while remaining > 0 {
        while let Some(v) = (0..num_nodes).find(|&v| alive[v] && outdeg[v] == 0) {
            remove(v, &mut alive, &mut indeg, &mut outdeg);
            remaining -= 1;
            right.push(v);
        }
        while let Some(v) = (0..num_nodes).find(|&v| alive[v] && indeg[v] == 0) {
            remove(v, &mut alive, &mut indeg, &mut outdeg);
            remaining -= 1;
            left.push(v);
        }
        let max_delta = (0..num_nodes)
            .filter(|&v| alive[v])
            .max_by_key(|&v| (outdeg[v] as isize - indeg[v] as isize, std::cmp::Reverse(v)));
        if let Some(v) = max_delta {
            remove(v, &mut alive, &mut indeg, &mut outdeg);
            remaining -= 1;
            left.push(v);
        }
    }

    let mut order = vec![0usize; num_nodes];
    for (i, &v) in left.iter().chain(right.iter().rev()).enumerate() {
        order[v] = i;
    }
    edges
        .iter()
        .map(|&(src, dst)| order[src] > order[dst])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::break_cycles;

    /// Whether applying the reversals yields a graph admitting a topological
    /// order.
    fn acyclic_after_reversal(num_nodes: usize, edges: &[(usize, usize)]) -> bool {
        let reversed = break_cycles(num_nodes, edges);
        let mut indeg = vec![0usize; num_nodes];
        let mut out_adj = vec![Vec::new(); num_nodes];
        for (&(src, dst), &rev) in edges.iter().zip(&reversed) {
            let (src, dst) = if rev { (dst, src) } else { (src, dst) };
            indeg[dst] += 1;
            out_adj[src].push(dst);
        }
        let mut stack: Vec<usize> = (0..num_nodes).filter(|&v| indeg[v] == 0).collect();
        let mut visited = 0;
        while let Some(v) = stack.pop() {
            visited += 1;
            for &w in &out_adj[v] {
                indeg[w] -= 1;
                if indeg[w] == 0 {
                    stack.push(w);
                }
            }
        }
        visited == num_nodes
    }

    #[test]
    fn dag_is_untouched() {
        let edges = [(0, 1), (0, 2), (1, 3), (2, 3)];
        assert!(break_cycles(4, &edges).iter().all(|&rev| !rev));
    }

    #[test]
    fn two_cycle_reverses_one_edge() {
        let edges = [(0, 1), (1, 0)];
        let reversed = break_cycles(2, &edges);
        assert_eq!(reversed.iter().filter(|&&rev| rev).count(), 1);
        assert!(acyclic_after_reversal(2, &edges));
    }

    #[test]
    fn three_cycle_reverses_one_edge() {
        let edges = [(0, 1), (1, 2), (2, 0)];
        let reversed = break_cycles(3, &edges);
        assert_eq!(reversed.iter().filter(|&&rev| rev).count(), 1);
        assert!(acyclic_after_reversal(3, &edges));
    }

    #[test]
    fn overlapping_cycles_become_acyclic() {
        let edges = [(0, 1), (1, 2), (2, 0), (0, 3), (3, 4), (4, 0), (2, 4)];
        assert!(acyclic_after_reversal(5, &edges));
    }

    #[test]
    fn multi_edges_become_acyclic() {
        let edges = [(0, 1), (0, 1), (1, 0)];
        assert!(acyclic_after_reversal(2, &edges));
    }
}
