
use crate::field::Fr;
use crate::{TuringError, Result};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct SparseMatrix {
    pub rows: usize,
    pub cols: usize,
    entries: HashMap<(usize, usize), Fr>,
}

impl SparseMatrix {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            entries: HashMap::new(),
        }
    }

    pub fn square(n: usize) -> Self {
        Self::new(n, n)
    }

    pub fn identity(n: usize) -> Self {
        let mut m = Self::square(n);
        for i in 0..n {
            m.set(i, i, Fr::one());
        }
        m
    }

    pub fn zeros(n: usize) -> Self {
        Self::square(n)
    }

    pub fn nnz(&self) -> usize {
        self.entries.len()
    }

    pub fn set(&mut self, row: usize, col: usize, value: Fr) {
        if value.is_zero() {
            self.entries.remove(&(row, col));
        } else {
            self.entries.insert((row, col), value);
        }
    }

    pub fn get(&self, row: usize, col: usize) -> Fr {
        self.entries.get(&(row, col)).copied().unwrap_or(Fr::zero())
    }

    pub fn add_to(&mut self, row: usize, col: usize, value: Fr) {
        let current = self.get(row, col);
        self.set(row, col, current + value);
    }

    pub fn is_square(&self) -> bool {
        self.rows == self.cols
    }

    pub fn mul_vec(&self, x: &[Fr]) -> Result<Vec<Fr>> {
        if x.len() != self.cols {
            return Err(TuringError::DimensionMismatch {
                expected: self.cols,
                got: x.len(),
            });
        }

        let mut y = vec![Fr::zero(); self.rows];

        for (&(row, col), &val) in &self.entries {
            y[row] = y[row] + val * x[col];
        }

        Ok(y)
    }

    pub fn scale(&self, c: Fr) -> Self {
        let mut result = Self::new(self.rows, self.cols);
        for (&(row, col), &val) in &self.entries {
            result.set(row, col, c * val);
        }
        result
    }

    pub fn add(&self, other: &Self) -> Result<Self> {
        if self.rows != other.rows || self.cols != other.cols {
            return Err(TuringError::DimensionMismatch {
                expected: self.rows,
                got: other.rows,
            });
        }

        let mut result = self.clone();
        for (&(row, col), &val) in &other.entries {
            result.add_to(row, col, val);
        }
        Ok(result)
    }

    pub fn to_dense(&self) -> Vec<Vec<Fr>> {
        let mut dense = vec![vec![Fr::zero(); self.cols]; self.rows];
        for (&(row, col), &val) in &self.entries {
            dense[row][col] = val;
        }
        dense
    }

    pub fn from_dense(dense: &[Vec<Fr>]) -> Self {
        let rows = dense.len();
        let cols = if rows > 0 { dense[0].len() } else { 0 };
        let mut m = Self::new(rows, cols);

        for (i, row) in dense.iter().enumerate() {
            for (j, &val) in row.iter().enumerate() {
                if !val.is_zero() {
                    m.set(i, j, val);
                }
            }
        }
        m
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, usize, Fr)> + '_ {
        self.entries.iter().map(|(&(r, c), &v)| (r, c, v))
    }
}

#[derive(Clone, Debug)]
pub struct GraphLaplacian {
    pub matrix: SparseMatrix,
    pub degrees: Vec<Fr>,
}

impl GraphLaplacian {
    pub fn from_edges(n: usize, edges: &[(usize, usize, Fr)], undirected: bool) -> Self {
        let mut matrix = SparseMatrix::square(n);
        let mut degrees = vec![Fr::zero(); n];

        for &(from, to, weight) in edges {
            matrix.add_to(from, to, -weight);
            degrees[from] = degrees[from] + weight;

            if undirected && from != to {
                matrix.add_to(to, from, -weight);
                degrees[to] = degrees[to] + weight;
            }
        }

        for (i, &deg) in degrees.iter().enumerate() {
            matrix.add_to(i, i, deg);
        }

        Self { matrix, degrees }
    }

    pub fn from_adjacency(adjacency: &[Vec<Fr>]) -> Result<Self> {
        let n = adjacency.len();
        if n == 0 {
            return Ok(Self {
                matrix: SparseMatrix::square(0),
                degrees: vec![],
            });
        }

        for row in adjacency {
            if row.len() != n {
                return Err(TuringError::NotSquare {
                    rows: n,
                    cols: row.len(),
                });
            }
        }

        let mut edges = Vec::new();
        for i in 0..n {
            for j in 0..n {
                if !adjacency[i][j].is_zero() {
                    edges.push((i, j, adjacency[i][j]));
                }
            }
        }

        Ok(Self::from_edges(n, &edges, false))
    }

    pub fn dim(&self) -> usize {
        self.matrix.rows
    }

    pub fn apply(&self, x: &[Fr]) -> Result<Vec<Fr>> {
        self.matrix.mul_vec(x)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparse_identity() {
        let id = SparseMatrix::identity(3);
        assert_eq!(id.get(0, 0), Fr::one());
        assert_eq!(id.get(1, 1), Fr::one());
        assert_eq!(id.get(2, 2), Fr::one());
        assert_eq!(id.get(0, 1), Fr::zero());
        assert_eq!(id.nnz(), 3);
    }

    #[test]
    fn test_mul_vec_identity() {
        let id = SparseMatrix::identity(3);
        let x = vec![Fr::from_u64(1), Fr::from_u64(2), Fr::from_u64(3)];
        let y = id.mul_vec(&x).unwrap();
        assert_eq!(y, x);
    }

    #[test]
    fn test_laplacian_simple() {
        let edges = vec![
            (0, 1, Fr::one()),
            (1, 2, Fr::one()),
        ];
        let lap = GraphLaplacian::from_edges(3, &edges, true);

        assert_eq!(lap.degrees[0], Fr::one());
        assert_eq!(lap.degrees[1], Fr::from_u64(2));
        assert_eq!(lap.degrees[2], Fr::one());

        assert_eq!(lap.matrix.get(0, 0), Fr::one());
        assert_eq!(lap.matrix.get(0, 1), -Fr::one());
        assert_eq!(lap.matrix.get(1, 1), Fr::from_u64(2));
    }

    #[test]
    fn test_laplacian_constant_vector() {
        let edges = vec![
            (0, 1, Fr::one()),
            (1, 2, Fr::one()),
            (0, 2, Fr::one()),
        ];
        let lap = GraphLaplacian::from_edges(3, &edges, true);

        let ones = vec![Fr::one(); 3];
        let result = lap.apply(&ones).unwrap();

        for r in result {
            assert_eq!(r, Fr::zero());
        }
    }
}
