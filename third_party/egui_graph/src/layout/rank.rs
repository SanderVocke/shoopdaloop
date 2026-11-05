//! Layer assignment.

/// Assign every vertex to a layer such that `layer[src] < layer[dst]` holds
/// for every edge.
///
/// Longest-path layering via a topological pass, followed by a pass pulling
/// every source down to just above its shallowest successor (avoiding the
/// long edges that longest-path otherwise produces from sources). The input
/// must be acyclic.
pub(super) fn assign_layers(num_nodes: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut out_adj: Vec<Vec<usize>> = vec![Vec::new(); num_nodes];
    let mut indeg = vec![0usize; num_nodes];
    for &(src, dst) in edges {
        out_adj[src].push(dst);
        indeg[dst] += 1;
    }

    let mut layer = vec![0usize; num_nodes];
    let mut pending = indeg.clone();
    let mut stack: Vec<usize> = (0..num_nodes).filter(|&v| indeg[v] == 0).collect();
    let mut visited = 0;
    while let Some(v) = stack.pop() {
        visited += 1;
        for &w in &out_adj[v] {
            layer[w] = layer[w].max(layer[v] + 1);
            pending[w] -= 1;
            if pending[w] == 0 {
                stack.push(w);
            }
        }
    }
    debug_assert_eq!(visited, num_nodes, "input must be acyclic");

    // Pull each source down to just above its shallowest successor.
    for v in 0..num_nodes {
        if indeg[v] == 0 {
            if let Some(min_succ) = out_adj[v].iter().map(|&w| layer[w]).min() {
                layer[v] = min_succ - 1;
            }
        }
    }
    layer
}

#[cfg(test)]
mod tests {
    use super::assign_layers;

    #[test]
    fn chain() {
        let edges = [(0, 1), (1, 2), (2, 3)];
        assert_eq!(assign_layers(4, &edges), vec![0, 1, 2, 3]);
    }

    #[test]
    fn diamond() {
        let edges = [(0, 1), (0, 2), (1, 3), (2, 3)];
        assert_eq!(assign_layers(4, &edges), vec![0, 1, 1, 2]);
    }

    #[test]
    fn source_pulled_toward_successors() {
        // `3` feeds only the deepest node and should sit just above it.
        let edges = [(0, 1), (1, 2), (3, 2)];
        assert_eq!(assign_layers(4, &edges), vec![0, 1, 2, 1]);
    }

    #[test]
    fn edges_always_point_to_deeper_layers() {
        let edges = [(0, 2), (1, 2), (2, 3), (1, 3), (3, 4), (0, 4)];
        let layer = assign_layers(5, &edges);
        for &(src, dst) in &edges {
            assert!(layer[src] < layer[dst]);
        }
    }
}
