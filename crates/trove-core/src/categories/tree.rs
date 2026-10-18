use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::pins::Pins;

/// A community of this run's Leiden hierarchy.
pub(super) struct Community {
    pub(super) parent: Option<usize>,
    /// Its notes, subcategories included.
    pub(super) members: BTreeSet<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Place {
    /// A community no pin claims, by its Leiden cluster id.
    Community(usize),
    /// A pinned category, by name.
    Pinned(String),
}

/// The categories once the pins are applied.
pub(super) struct Tree {
    /// Every category with a note, each parent before its subcategories, with its parent.
    pub(super) places: Vec<(Place, Option<Place>)>,
    /// The notes in each category and in none of its subcategories.
    pub(super) direct: BTreeMap<Place, BTreeSet<i64>>,
    /// The notes in each category, subcategories included.
    pub(super) members: BTreeMap<Place, BTreeSet<i64>>,
}

/// Applies the pins to the communities (by `(level, cluster id)`), whose notes are each in the community `deepest` gives.
/// A pinned category takes every community most of whose notes are among its own, and the hand-placed notes.
pub(super) fn tree(
    communities: &BTreeMap<(u32, usize), Community>,
    deepest: &BTreeMap<i64, usize>,
    pins: &Pins,
) -> Tree {
    let mut children: HashMap<usize, Vec<&BTreeSet<i64>>> = HashMap::new();
    for community in communities.values() {
        if let Some(parent) = community.parent {
            children.entry(parent).or_default().push(&community.members);
        }
    }

    // Communities come parents first, so a community's ancestors are placed before it.
    let mut place_of: HashMap<usize, Place> = HashMap::new();
    let mut pinned_above: HashMap<usize, BTreeSet<String>> = HashMap::new();
    let mut parents: BTreeMap<Place, Option<Place>> = BTreeMap::new();
    for (&(_, id), community) in communities {
        let mut above = BTreeSet::new();
        if let Some(parent) = community.parent {
            above.clone_from(&pinned_above[&parent]);
            if let Place::Pinned(name) = &place_of[&parent] {
                above.insert(name.clone());
            }
        }

        let subcategories = children.get(&id).map_or(&[][..], Vec::as_slice);
        let place = claiming_pin(&community.members, subcategories, pins, &above)
            .map_or(Place::Community(id), |name| Place::Pinned(name.to_owned()));
        // A pinned category without a parent pin sits where its shallowest community does.
        parents
            .entry(place.clone())
            .or_insert_with(|| community.parent.map(|parent| place_of[&parent].clone()));
        place_of.insert(id, place);
        pinned_above.insert(id, above);
    }
    for name in pins.anchors.keys() {
        let place = Place::Pinned(name.clone());
        match pins.parents.get(name) {
            Some(parent) => {
                parents.insert(place, parent.clone().map(Place::Pinned));
            }
            None => {
                parents.entry(place).or_insert(None);
            }
        }
    }
    break_loops(&mut parents);

    let mut direct: BTreeMap<Place, BTreeSet<i64>> = BTreeMap::new();
    for (file_id, cluster) in deepest {
        if !pins.placed.contains_key(file_id) {
            direct
                .entry(place_of[cluster].clone())
                .or_default()
                .insert(*file_id);
        }
    }
    for (&file_id, name) in &pins.placed {
        direct
            .entry(Place::Pinned(name.clone()))
            .or_default()
            .insert(file_id);
    }

    let mut members: BTreeMap<Place, BTreeSet<i64>> = BTreeMap::new();
    for (place, notes) in &direct {
        let mut at = Some(place);
        while let Some(place) = at {
            members.entry(place.clone()).or_default().extend(notes);
            at = parents[place].as_ref();
        }
    }

    // A community whose other notes were pinned elsewhere can be left holding one subcategory, which then replaces it.
    let hollow: Vec<Place> = members
        .keys()
        .filter(|place| matches!(place, Place::Community(_)) && !direct.contains_key(place))
        .cloned()
        .collect();
    for place in hollow {
        let children: Vec<&Place> = members
            .keys()
            .filter(|child| parents[*child].as_ref() == Some(&place))
            .collect();
        if let [child] = children[..] {
            parents.insert(child.clone(), parents[&place].clone());
            members.remove(&place);
        }
    }

    let depth = |place: &Place| {
        let mut depth = 0;
        let mut at = &parents[place];
        while let Some(parent) = at {
            depth += 1;
            at = &parents[parent];
        }
        depth
    };
    let mut places: Vec<(Place, Option<Place>)> = members
        .keys()
        .map(|place| (place.clone(), parents[place].clone()))
        .collect();
    places.sort_by_cached_key(|(place, _)| (depth(place), place.clone()));

    Tree {
        places,
        direct,
        members,
    }
}

/// The pinned category claiming a community: of those holding most of its notes and most of each of its
/// subcategories' notes, the one closest to it (Jaccard), then the first by name. A pin claiming an ancestor of the
/// community can't claim it too.
fn claiming_pin<'a>(
    members: &BTreeSet<i64>,
    subcategories: &[&BTreeSet<i64>],
    pins: &'a Pins,
    pinned_above: &BTreeSet<String>,
) -> Option<&'a str> {
    let mostly_in = |notes: &BTreeSet<i64>, anchor: &BTreeSet<i64>| {
        notes.intersection(anchor).count() * 2 > notes.len()
    };

    pins.anchors
        .iter()
        .filter(|(name, anchor)| {
            !pinned_above.contains(*name)
                && mostly_in(members, anchor)
                && subcategories.iter().all(|notes| mostly_in(notes, anchor))
        })
        .map(|(name, anchor)| {
            let shared = members.intersection(anchor).count();
            let union = members.len() + anchor.len() - shared;
            (shared as f64 / union as f64, name)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0).then(b.1.cmp(a.1)))
        .map(|(_, name)| name.as_str())
}

/// Parent pins can make a loop (each of two categories under the other); a category whose parents lead back to it is
/// moved to the top level.
fn break_loops(parents: &mut BTreeMap<Place, Option<Place>>) {
    let places: Vec<Place> = parents.keys().cloned().collect();
    for place in places {
        let mut seen = BTreeSet::new();
        let mut at = parents[&place].clone();
        while let Some(parent) = at {
            if parent == place {
                parents.insert(place, None);
                break;
            }
            if !seen.insert(parent.clone()) {
                break;
            }
            at = parents[&parent].clone();
        }
    }
}
