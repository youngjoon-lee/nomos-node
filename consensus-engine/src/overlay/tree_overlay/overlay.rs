use super::tree::Tree;
use crate::{overlay::LeaderSelection, Committee, NodeId, Overlay};
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;

pub trait Shuffler {
    fn shuffle<R>(&self, nodes: &mut [NodeId], rng: &mut R)
    where
        R: rand::Rng + ?Sized;
}

#[derive(Debug, Default, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DefaultShuffler;

impl Shuffler for DefaultShuffler {
    fn shuffle<R>(&self, nodes: &mut [NodeId], rng: &mut R)
    where
        R: rand::Rng + ?Sized,
    {
        <[NodeId] as rand::seq::SliceRandom>::shuffle(nodes, rng)
    }
}

#[derive(Debug, Clone)]
pub struct TreeOverlaySettings<L: LeaderSelection, S = DefaultShuffler> {
    pub nodes: Vec<NodeId>,
    pub current_leader: NodeId,
    pub entropy: [u8; 32],
    pub number_of_committees: usize,
    pub leader: L,
    pub shuffer: S,
}

#[derive(Debug, Clone)]
pub struct TreeOverlay<L, S = DefaultShuffler> {
    pub(super) entropy: [u8; 32],
    pub(super) number_of_committees: usize,
    pub(super) nodes: Vec<NodeId>,
    pub(super) current_leader: NodeId,
    pub(super) carnot_tree: Tree,
    pub(super) leader: L,
    pub(super) shuffer: S,
}

impl<L, S> Overlay for TreeOverlay<L, S>
where
    L: LeaderSelection + Send + Sync + 'static,
    S: Shuffler + Clone + Send + Sync + 'static,
{
    type Settings = TreeOverlaySettings<L, S>;

    type LeaderSelection = L;

    fn new(settings: Self::Settings) -> Self {
        let TreeOverlaySettings {
            mut nodes,
            current_leader,
            entropy,
            number_of_committees,
            leader,
            shuffer,
        } = settings;
        let mut rng = StdRng::from_seed(entropy);
        shuffer.shuffle(&mut nodes, &mut rng);

        let carnot_tree = Tree::new(&nodes, number_of_committees);

        Self {
            entropy,
            number_of_committees,
            nodes,
            current_leader,
            carnot_tree,
            leader,
            shuffer,
        }
    }

    fn root_committee(&self) -> Committee {
        self.carnot_tree.root_committee().clone()
    }

    fn rebuild(&mut self, _timeout_qc: crate::TimeoutQc) {
        unimplemented!("do nothing for now")
    }

    fn is_member_of_child_committee(&self, parent: NodeId, child: NodeId) -> bool {
        let child_parent = self.parent_committee(child);
        let parent = self.carnot_tree.committee_by_member_id(&parent);
        parent.map_or(false, |p| child_parent.eq(p))
    }

    fn is_member_of_root_committee(&self, id: NodeId) -> bool {
        self.carnot_tree.root_committee().contains(&id)
    }

    fn is_member_of_leaf_committee(&self, id: NodeId) -> bool {
        self.carnot_tree
            .leaf_committees()
            .values()
            .any(|committee| committee.contains(&id))
    }

    fn is_child_of_root_committee(&self, id: NodeId) -> bool {
        self.parent_committee(id) == self.root_committee()
    }

    fn parent_committee(&self, id: NodeId) -> Committee {
        self.carnot_tree.parent_committee_from_member_id(&id)
    }

    fn child_committees(&self, id: NodeId) -> Vec<Committee> {
        // Lookup committee index by member id, then committee id by index.
        self.carnot_tree
            .committees_by_member
            .get(&id)
            .and_then(|committee_idx| self.carnot_tree.inner_committees.get(*committee_idx))
            .map(|committee_id| {
                let (l, r) = self.carnot_tree.child_committees(committee_id);
                let extract_committee = |committee_id| {
                    self.carnot_tree
                        .committee_id_to_index
                        .get(committee_id)
                        .and_then(|committee_idx| {
                            self.carnot_tree.membership_committees.get(committee_idx)
                        })
                };
                let l = l.and_then(extract_committee).into_iter().cloned();
                let r = r.and_then(extract_committee).into_iter().cloned();
                l.chain(r).collect()
            })
            .expect("NodeId not found in overlay")
    }

    fn leaf_committees(&self, _id: NodeId) -> Vec<Committee> {
        self.carnot_tree
            .leaf_committees()
            .into_values()
            .cloned()
            .collect()
    }

    fn node_committee(&self, id: NodeId) -> Committee {
        self.carnot_tree
            .committees_by_member
            .get(&id)
            .and_then(|committee_index| self.carnot_tree.membership_committees.get(committee_index))
            .cloned()
            .unwrap_or_default()
    }

    fn next_leader(&self) -> NodeId {
        let mut rng = StdRng::from_seed(self.entropy);
        *self.nodes.choose(&mut rng).unwrap()
    }

    fn super_majority_threshold(&self, id: NodeId) -> usize {
        if self.is_member_of_leaf_committee(id) {
            return 0;
        }
        self.carnot_tree
            .committee_by_member_id(&id)
            .map(|c| (c.len() * 2 / 3) + 1)
            .expect("node is not part of any committee")
    }

    fn leader_super_majority_threshold(&self, _id: NodeId) -> usize {
        let root_committee = &self.carnot_tree.inner_committees[0];
        let children = self.carnot_tree.child_committees(root_committee);
        let children_size = children.0.map_or(0, |c| {
            self.carnot_tree
                .committee_by_committee_id(c)
                .map_or(0, |c| c.len())
        }) + children.1.map_or(0, |c| {
            self.carnot_tree
                .committee_by_committee_id(c)
                .map_or(0, |c| c.len())
        });
        let root_size = self.root_committee().len();
        let committee_size = root_size + children_size;
        (committee_size * 2 / 3) + 1
    }

    fn update_leader_selection<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(Self::LeaderSelection) -> Result<Self::LeaderSelection, E>,
    {
        match f(self.leader.clone()) {
            Ok(leader_selection) => Ok(Self {
                leader: leader_selection,
                ..self.clone()
            }),
            Err(e) => Err(e),
        }
    }
}

impl<L, S> TreeOverlay<L, S>
where
    L: LeaderSelection + Send + Sync + 'static,
    S: Shuffler + Clone + Send + Sync + 'static,
{
    pub fn advance(&self, entropy: [u8; 32], leader: L) -> Self {
        Self::new(TreeOverlaySettings {
            nodes: self.nodes.clone(),
            current_leader: self.next_leader(),
            entropy,
            number_of_committees: self.number_of_committees,
            leader,
            shuffer: self.shuffer.clone(),
        })
    }

    pub fn is_leader(&self, id: &NodeId) -> bool {
        id == &self.current_leader
    }

    pub fn leader(&self) -> &NodeId {
        &self.current_leader
    }
}

#[cfg(test)]
mod tests {
    use crate::overlay::RoundRobin;
    use crate::Overlay;

    use super::*;

    #[test]
    fn test_carnot_overlay_leader() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        assert_eq!(*overlay.leader(), nodes[0]);
    }

    #[test]
    fn test_next_leader_is_advance_current_leader() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let mut overlay = TreeOverlay::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        let leader = overlay.next_leader();
        overlay = overlay.advance([1; 32], RoundRobin::new());

        assert_eq!(leader, *overlay.leader());
    }

    #[test]
    fn test_root_committee() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::<_, DefaultShuffler>::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        let mut expected_root = Committee::new();
        expected_root.insert(overlay.nodes[9]);
        expected_root.extend(overlay.nodes[0..3].iter());

        assert_eq!(overlay.root_committee(), expected_root);
    }

    #[test]
    fn test_leaf_committees() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::<_, DefaultShuffler>::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        let mut leaf_committees = overlay
            .leaf_committees(NodeId::new([0; 32]))
            .into_iter()
            .map(|s| {
                let mut vec = s.into_iter().collect::<Vec<_>>();
                vec.sort();
                vec
            })
            .collect::<Vec<_>>();
        leaf_committees.sort();
        let mut c1 = overlay.nodes[3..6].to_vec();
        c1.sort();
        let mut c2 = overlay.nodes[6..9].to_vec();
        c2.sort();
        let mut expected = vec![c1, c2];
        expected.sort();
        assert_eq!(leaf_committees, expected);
    }

    #[test]
    fn test_super_majority_threshold_for_leaf() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::<_, DefaultShuffler>::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        assert_eq!(overlay.super_majority_threshold(overlay.nodes[8]), 0);
    }

    #[test]
    fn test_super_majority_threshold_for_root_member() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::<_, DefaultShuffler>::new(TreeOverlaySettings {
            current_leader: nodes[0],
            nodes,
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        assert_eq!(overlay.super_majority_threshold(overlay.nodes[0]), 3);
    }

    #[test]
    fn test_leader_super_majority_threshold() {
        let nodes: Vec<_> = (0..10).map(|i| NodeId::new([i as u8; 32])).collect();
        let overlay = TreeOverlay::<_, DefaultShuffler>::new(TreeOverlaySettings {
            nodes: nodes.clone(),
            current_leader: nodes[0],
            entropy: [0; 32],
            number_of_committees: 3,
            leader: RoundRobin::new(),
            shuffer: DefaultShuffler,
        });

        assert_eq!(
            overlay.leader_super_majority_threshold(NodeId::new([0; 32])),
            7
        );
    }
}
