//! The conversion graph: formats are nodes, engine conversions are edges.
//! When no engine converts directly, Joey chains up to three steps
//! (e.g. WebP → PNG → PDF).

use std::collections::{BTreeSet, HashMap, VecDeque};

use crate::format::Format;

pub const MAX_STEPS: usize = 3;

#[derive(Default)]
pub struct Graph {
    /// (from, to) → indices of engines able to do it, in preference order.
    edges: HashMap<(Format, Format), Vec<usize>>,
}

impl Graph {
    pub fn add(&mut self, from: Format, to: Format, engine: usize) {
        self.edges.entry((from, to)).or_default().push(engine);
    }

    pub fn engines_for(&self, from: Format, to: Format) -> &[usize] {
        self.edges
            .get(&(from, to))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    fn next(&self, from: Format) -> impl Iterator<Item = Format> + '_ {
        Format::ALL
            .iter()
            .copied()
            .filter(move |&to| self.edges.contains_key(&(from, to)))
    }

    /// Shortest chain of formats from `from` to `to`, both included.
    pub fn route(&self, from: Format, to: Format) -> Option<Vec<Format>> {
        if from == to {
            return None;
        }
        let mut prev: HashMap<Format, Format> = HashMap::new();
        let mut queue = VecDeque::from([(from, 0usize)]);
        while let Some((node, depth)) = queue.pop_front() {
            if depth == MAX_STEPS {
                continue;
            }
            for nxt in self.next(node) {
                if nxt == from || prev.contains_key(&nxt) {
                    continue;
                }
                prev.insert(nxt, node);
                if nxt == to {
                    let mut path = vec![to];
                    let mut cur = to;
                    while let Some(&p) = prev.get(&cur) {
                        path.push(p);
                        cur = p;
                    }
                    path.reverse();
                    return Some(path);
                }
                queue.push_back((nxt, depth + 1));
            }
        }
        None
    }

    /// Every format reachable from `from`, in the order formats are listed.
    pub fn targets(&self, from: Format) -> Vec<Format> {
        let mut seen = BTreeSet::new();
        let mut queue = VecDeque::from([(from, 0usize)]);
        while let Some((node, depth)) = queue.pop_front() {
            if depth == MAX_STEPS {
                continue;
            }
            for nxt in self.next(node) {
                if nxt != from && seen.insert(nxt) {
                    queue.push_back((nxt, depth + 1));
                }
            }
        }
        seen.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_multi_step_route() {
        let mut g = Graph::default();
        g.add(Format::Webp, Format::Png, 0);
        g.add(Format::Png, Format::Pdf, 1);
        assert_eq!(
            g.route(Format::Webp, Format::Pdf),
            Some(vec![Format::Webp, Format::Png, Format::Pdf])
        );
        assert_eq!(g.targets(Format::Webp), vec![Format::Png, Format::Pdf]);
        assert_eq!(g.route(Format::Pdf, Format::Webp), None);
    }

    #[test]
    fn prefers_direct_route() {
        let mut g = Graph::default();
        g.add(Format::Jpg, Format::Png, 0);
        g.add(Format::Png, Format::Pdf, 0);
        g.add(Format::Jpg, Format::Pdf, 1);
        assert_eq!(
            g.route(Format::Jpg, Format::Pdf),
            Some(vec![Format::Jpg, Format::Pdf])
        );
    }

    #[test]
    fn stops_after_three_steps() {
        let mut g = Graph::default();
        let chain = [
            Format::Heic,
            Format::Tiff,
            Format::Png,
            Format::Jpg,
            Format::Pdf,
        ];
        for w in chain.windows(2) {
            g.add(w[0], w[1], 0);
        }
        assert!(g.route(Format::Heic, Format::Jpg).is_some());
        assert!(g.route(Format::Heic, Format::Pdf).is_none());
    }
}
