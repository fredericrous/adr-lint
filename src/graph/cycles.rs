//! Strongly connected components of the edges among the children of one
//! module, and the shortest cycles that cover them.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Where an import is written: file, byte offset, the path as written.
pub(super) type Site = (String, usize, String);

pub(super) type Edges = BTreeMap<(String, String), BTreeSet<Site>>;

pub(super) struct Component {
    pub(super) members: Vec<String>,
    pub(super) edges: Edges,
    /// Each cycle as its nodes in order, the first node repeated last.
    pub(super) cycles: Vec<Vec<String>>,
}

pub(super) fn components(edges: &Edges) -> Vec<Component> {
    let mut successors: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (from, to) in edges.keys() {
        successors.entry(from).or_default().insert(to);
        successors.entry(to).or_default();
    }
    let reach: BTreeMap<&str, BTreeSet<&str>> = successors
        .keys()
        .map(|node| (*node, reachable(&successors, node)))
        .collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut found = Vec::new();
    for node in successors.keys() {
        if seen.contains(node) {
            continue;
        }
        let members: BTreeSet<&str> = reach[node]
            .iter()
            .filter(|other| reach[*other].contains(node))
            .copied()
            .collect();
        seen.extend(members.iter().copied());
        if members.len() > 1 {
            found.push(component(edges, &members));
        }
    }
    found
}

fn reachable<'a>(
    successors: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    start: &'a str,
) -> BTreeSet<&'a str> {
    let mut reached = BTreeSet::from([start]);
    let mut pending = vec![start];
    while let Some(node) = pending.pop() {
        for next in &successors[node] {
            if reached.insert(next) {
                pending.push(next);
            }
        }
    }
    reached
}

fn component(edges: &Edges, members: &BTreeSet<&str>) -> Component {
    let inside: Edges = edges
        .iter()
        .filter(|((from, to), _)| members.contains(from.as_str()) && members.contains(to.as_str()))
        .map(|(edge, sites)| (edge.clone(), sites.clone()))
        .collect();
    let mut covered: BTreeSet<(String, String)> = BTreeSet::new();
    let mut cycles = Vec::new();
    for (from, to) in inside.keys() {
        if covered.contains(&(from.clone(), to.clone())) {
            continue;
        }
        if let Some(path) = shortest_path(&inside, to, from) {
            let mut cycle = vec![from.clone()];
            cycle.extend(path);
            for pair in cycle.windows(2) {
                covered.insert((pair[0].clone(), pair[1].clone()));
            }
            cycles.push(cycle);
        }
    }
    Component {
        members: members.iter().map(|member| (*member).to_string()).collect(),
        edges: inside,
        cycles,
    }
}

/// The nodes of a shortest path from `start` to `goal`, both included.
fn shortest_path(edges: &Edges, start: &str, goal: &str) -> Option<Vec<String>> {
    let mut previous: BTreeMap<&str, &str> = BTreeMap::new();
    let mut queue = VecDeque::from([start]);
    let mut visited = BTreeSet::from([start]);
    while let Some(node) = queue.pop_front() {
        if node == goal {
            let mut path = vec![goal.to_string()];
            let mut current = goal;
            while let Some(before) = previous.get(current) {
                path.push((*before).to_string());
                current = before;
            }
            path.reverse();
            return Some(path);
        }
        for (_, to) in edges.keys().filter(|(from, _)| from == node) {
            if visited.insert(to) {
                previous.insert(to, node);
                queue.push_back(to);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edges(pairs: &[(&str, &str)]) -> Edges {
        pairs
            .iter()
            .map(|(from, to)| ((from.to_string(), to.to_string()), BTreeSet::new()))
            .collect()
    }

    #[test]
    fn a_ring_is_one_component_with_one_cycle() {
        let found = components(&edges(&[("a", "b"), ("b", "c"), ("c", "a"), ("c", "d")]));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].members, ["a", "b", "c"]);
        assert_eq!(found[0].cycles, [["a", "b", "c", "a"]]);
    }

    #[test]
    fn an_extra_edge_adds_a_cycle() {
        let found = components(&edges(&[("a", "b"), ("b", "a"), ("b", "c"), ("c", "a")]));
        assert_eq!(
            found[0].cycles,
            [vec!["a", "b", "a"], vec!["b", "c", "a", "b"]]
        );
    }

    #[test]
    fn an_acyclic_graph_has_none() {
        assert!(components(&edges(&[("a", "b"), ("b", "c")])).is_empty());
    }
}
