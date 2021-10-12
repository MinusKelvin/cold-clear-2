use std::ops::{Index, IndexMut};
use std::sync::atomic::{self, AtomicBool};

use enum_map::EnumMap;
use enumset::EnumSet;
use rand::prelude::*;
use smallvec::SmallVec;

use crate::data::{GameState, Piece, Placement};
use crate::map::StateMap;

use super::{
    update_child, BackpropUpdate, Child, ChildData, Evaluation, LayerCommon, SelectResult,
};

#[derive(Default)]
pub(super) struct Layer<E: Evaluation> {
    pub states: StateMap<Node<E>>,
}

pub(super) struct Node<E: Evaluation> {
    pub parents: SmallVec<[(u64, Placement, Piece); 1]>,
    pub eval: E,
    pub children: Option<PackedChildren<E>>,
    pub expanding: AtomicBool,
    // we need this info while backpropagating, but we don't have access to the game state then
    bag: EnumSet<Piece>,
}

impl<E: Evaluation> Layer<E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, || Node {
            parents: SmallVec::new(),
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
            bag: root.bag,
        });
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        let node = self.states.get(&state).unwrap();
        let children = match &node.children {
            Some(children) => children,
            None => return vec![],
        };

        let mut candidates: Vec<&_> = vec![];
        for piece in state.bag {
            candidates.extend(children[piece].first());
        }
        candidates.sort_by(|a, b| a.cached_eval.partial_cmp(&b.cached_eval).unwrap().reverse());

        candidates.into_iter().map(|c| c.mv).collect()
    }

    pub fn select(&self, game_state: &GameState) -> SelectResult {
        let node = self
            .states
            .get(&game_state)
            .expect("Link to non-existent node?");

        let children = match &node.children {
            None => {
                if node.expanding.swap(true, atomic::Ordering::Relaxed) {
                    return SelectResult::Failed;
                } else {
                    return SelectResult::Done;
                }
            }
            Some(children) => children,
        };

        let next = game_state
            .bag
            .iter()
            .nth(thread_rng().gen_range(0..game_state.bag.len()))
            .unwrap();

        if children[next].is_empty() {
            return SelectResult::Failed;
        }

        loop {
            let s: f64 = thread_rng().gen();
            let i = -s.log2() as usize;
            if i < children[next].len() {
                break SelectResult::Advance(next, children[next][i].mv);
            }
        }
    }

    pub fn get_eval(&self, raw: u64) -> E {
        self.states.get_raw(raw).unwrap().eval
    }

    pub fn create_node(&self, child: &ChildData<E>, parent: u64, speculation_piece: Piece) -> E {
        let mut node = self
            .states
            .get_or_insert_with(&child.resulting_state, || Node {
                parents: SmallVec::new(),
                eval: child.eval,
                children: None,
                expanding: AtomicBool::new(false),
                bag: child.resulting_state.bag,
            });
        node.parents.push((parent, child.mv, speculation_piece));
        node.eval
    }

    pub fn expand(
        &self,
        next_layer: &LayerCommon<E>,
        parent_state: GameState,
        children: EnumMap<Piece, Vec<ChildData<E>>>,
    ) -> Vec<BackpropUpdate> {
        let mut childs_data = vec![];
        let mut childs_indices = [0; 8];

        // We need to acquire the lock on the parent since the backprop routine needs the children
        // lists to exist, and they won't if we're still creating them
        let parent_index = self.states.index(&parent_state);
        let mut parent = self.states.get_raw_mut(parent_index).unwrap();

        for speculation_piece in EnumSet::all() {
            for child in &children[speculation_piece] {
                let eval = next_layer
                    .kind
                    .create_node(&child, parent_index, speculation_piece);
                childs_data.push(Child {
                    mv: child.mv,
                    cached_eval: eval + child.reward,
                    reward: child.reward,
                });
            }
            childs_indices[speculation_piece as usize + 1] = childs_data.len() as u16;
        }

        let mut children = PackedChildren {
            data: childs_data.into_boxed_slice(),
            start_indices: childs_indices,
        };

        for p in EnumSet::all() {
            children[p].sort_by(|a, b| a.cached_eval.cmp(&b.cached_eval).reverse());
        }

        let next_possibilities = parent.bag;
        parent.eval = E::average(
            next_possibilities
                .iter()
                .map(|p| children[p].first().map(|c| c.cached_eval)),
        );

        parent.children = Some(children);

        let mut next = vec![];

        for &(grandparent, mv, speculation_piece) in &parent.parents {
            next.push(BackpropUpdate {
                parent: grandparent,
                mv,
                speculation_piece,
                child: parent_index,
            });
        }

        next
    }

    pub fn backprop(
        &self,
        to_update: Vec<BackpropUpdate>,
        next_layer: &LayerCommon<E>,
    ) -> Vec<BackpropUpdate> {
        let mut new_updates = vec![];

        for update in to_update {
            let mut parent = self.states.get_raw_mut(update.parent).unwrap();
            let child_eval = next_layer.kind.get_eval(update.child);

            let parent_bag = parent.bag;
            let children = parent.children.as_mut().unwrap();
            let list = &mut children[update.speculation_piece];

            let is_best = update_child(list, update.mv, child_eval);

            if is_best {
                let best_for = |p: Piece| children[p].first().map(|c| c.cached_eval);

                let eval = E::average(parent_bag.iter().map(best_for));

                if parent.eval != eval {
                    parent.eval = eval;

                    for &(parent, mv, speculation_piece) in &parent.parents {
                        new_updates.push(BackpropUpdate {
                            parent,
                            mv,
                            speculation_piece,
                            child: update.parent,
                        });
                    }
                }
            }
        }

        new_updates
    }
}

pub(super) struct PackedChildren<E: Evaluation> {
    data: Box<[Child<E>]>,
    start_indices: [u16; 8],
}

impl<E: Evaluation> Index<Piece> for PackedChildren<E> {
    type Output = [Child<E>];

    fn index(&self, index: Piece) -> &Self::Output {
        let start = self.start_indices[index as usize] as usize;
        let end = self.start_indices[index as usize + 1] as usize;
        &self.data[start..end]
    }
}

impl<E: Evaluation> IndexMut<Piece> for PackedChildren<E> {
    fn index_mut(&mut self, index: Piece) -> &mut Self::Output {
        let start = self.start_indices[index as usize] as usize;
        let end = self.start_indices[index as usize + 1] as usize;
        &mut self.data[start..end]
    }
}
