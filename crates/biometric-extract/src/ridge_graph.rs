
use crate::error::Result;
use crate::image_proc::BinaryImage;
use crate::minutiae::MinutiaeSet;
use turing_core::{Fr, GraphLaplacian};
use tracing::debug;

#[derive(Debug, Clone)]
pub struct RidgeEdge {
    pub from: usize,
    pub to: usize,
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub struct RidgeGraph {
    pub node_count: usize,
    pub edges: Vec<RidgeEdge>,
}

impl RidgeGraph {
    pub fn build(minutiae: &MinutiaeSet, _image: &BinaryImage) -> Result<Self> {
        Ok(Self::from_minutiae(minutiae))
    }

    pub fn from_minutiae(minutiae: &MinutiaeSet) -> Self {
        let n = minutiae.len();
        let mut edges = Vec::new();

        let max_distance = 100.0;
        let k_nearest = 5;

        for i in 0..n {
            let mut distances: Vec<(usize, f64)> = (0..n)
                .filter(|&j| j != i)
                .map(|j| {
                    let dist = minutiae.minutiae[i].distance_to(&minutiae.minutiae[j]);
                    (j, dist)
                })
                .filter(|(_, d)| *d < max_distance)
                .collect();

            distances.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            for (j, dist) in distances.iter().take(k_nearest) {
                let weight = 1.0 - (dist / max_distance);

                if i < *j {
                    edges.push(RidgeEdge {
                        from: i,
                        to: *j,
                        weight,
                    });
                }
            }
        }

        debug!("Built ridge graph: {} nodes, {} edges", n, edges.len());

        Self {
            node_count: n,
            edges,
        }
    }

    pub fn from_minutiae_with_orientation(minutiae: &MinutiaeSet) -> Self {
        let n = minutiae.len();
        let mut edges = Vec::new();

        let max_distance = 100.0;
        let k_nearest = 5;

        for i in 0..n {
            let mi = &minutiae.minutiae[i];

            let mut candidates: Vec<(usize, f64, f64)> = (0..n)
                .filter(|&j| j != i)
                .map(|j| {
                    let mj = &minutiae.minutiae[j];
                    let dist = mi.distance_to(mj);

                    let dx = mj.x - mi.x;
                    let dy = mj.y - mi.y;
                    let dir = dy.atan2(dx);

                    let compat_i = (mi.theta - dir).cos().abs();
                    let compat_j = (mj.theta - dir).cos().abs();
                    let angular_weight = (compat_i + compat_j) / 2.0;

                    (j, dist, angular_weight)
                })
                .filter(|(_, d, _)| *d < max_distance)
                .collect();

            candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

            for (j, dist, angular) in candidates.iter().take(k_nearest) {
                if i < *j {
                    let dist_weight = 1.0 - (dist / max_distance);
                    let weight = 0.7 * dist_weight + 0.3 * angular;

                    edges.push(RidgeEdge {
                        from: i,
                        to: *j,
                        weight,
                    });
                }
            }
        }

        Self {
            node_count: n,
            edges,
        }
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    pub fn to_laplacian(&self) -> GraphLaplacian {
        let edges: Vec<(usize, usize, Fr)> = self
            .edges
            .iter()
            .map(|e| {
                let scaled = ((e.weight * 1000.0) as u64).max(1);
                (e.from, e.to, Fr::from_u64(scaled))
            })
            .collect();

        GraphLaplacian::from_edges(self.node_count, &edges, true)
    }

    pub fn adjacency_matrix(&self) -> Vec<Vec<f64>> {
        let n = self.node_count;
        let mut adj = vec![vec![0.0; n]; n];

        for edge in &self.edges {
            adj[edge.from][edge.to] = edge.weight;
            adj[edge.to][edge.from] = edge.weight;
        }

        adj
    }

    pub fn degrees(&self) -> Vec<f64> {
        let mut deg = vec![0.0; self.node_count];

        for edge in &self.edges {
            deg[edge.from] += edge.weight;
            deg[edge.to] += edge.weight;
        }

        deg
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_graph() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);

        assert_eq!(graph.node_count, 8);
        assert!(graph.edge_count() > 0);
    }

    #[test]
    fn test_laplacian_conversion() {
        let minutiae = MinutiaeSet::mock();
        let graph = RidgeGraph::from_minutiae(&minutiae);
        let laplacian = graph.to_laplacian();

        assert_eq!(laplacian.dim(), 8);
    }
}
