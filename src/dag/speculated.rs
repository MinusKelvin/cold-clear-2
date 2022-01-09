use std::ops::{Index, IndexMut};
use std::sync::atomic::{self, AtomicBool};

use bumpalo_herd::{Herd, Member};
use enum_map::EnumMap;
use enumset::EnumSet;
use rand::prelude::*;

use crate::data::{GameState, Piece, Placement};
use crate::map::StateMap;

use super::{
    update_child, BackpropUpdate, Child, ChildData, Evaluation, LayerCommon, SelectResult,
};

#[derive(Default)]
pub(super) struct Layer<'bump, E: Evaluation> {
    pub states: StateMap<Node<'bump, E>>,
}

pub(super) struct Node<'bump, E: Evaluation> {
    pub parents: &'bump [(u64, Placement, Piece)],
    pub eval: E,
    pub children: Option<PackedChildren<'bump, E>>,
    pub expanding: AtomicBool,
    // we need this info while backpropagating, but we don't have access to the game state then
    bag: EnumSet<Piece>,
}

impl<'bump, E: Evaluation> Layer<'bump, E> {
    pub fn initialize_root(&self, root: &GameState) {
        let _ = self.states.get_or_insert_with(root, || Node {
            parents: &[],
            eval: E::default(),
            children: None,
            expanding: AtomicBool::new(false),
            bag: root.bag,
        });
    }

    pub fn suggest(&self, state: &GameState) -> Vec<Placement> {
        puffin::profile_function!();
        let node = self.states.get(state).unwrap();
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

    pub fn select(&self, game_state: &GameState, exploration: f64) -> SelectResult {
        puffin::profile_function!();
        let node = self
            .states
            .get(game_state)
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

        let s: f64 = thread_rng().gen();
        let i = ((-s.ln() / exploration) % children[next].len() as f64) as usize;
        SelectResult::Advance(next, children[next][i].mv)
    }

    pub fn get_eval(&self, raw: u64) -> E {
        self.states.get_raw(raw).unwrap().eval
    }

    pub fn create_node(
        &self,
        bump: &Member<'bump>,
        child: &ChildData<E>,
        parent: u64,
        speculation_piece: Piece,
    ) -> E {
        let mut node = self
            .states
            .get_or_insert_with(&child.resulting_state, || Node {
                parents: &[],
                eval: child.eval,
                children: None,
                expanding: AtomicBool::new(false),
                bag: child.resulting_state.bag,
            });
        node.parents = bump.alloc_slice_fill_with(node.parents.len() + 1, |i| {
            node.parents
                .get(i)
                .copied()
                .unwrap_or((parent, child.mv, speculation_piece))
        });
        node.eval
    }

    pub fn expand(
        &self,
        herd: &'bump Herd,
        next_layer: &LayerCommon<E>,
        parent_state: GameState,
        children: EnumMap<Piece, Vec<ChildData<E>>>,
    ) -> Vec<BackpropUpdate> {
        puffin::profile_function!();
        let mut childs_data = vec![];
        let mut childs_indices = [0; 8];

        // We need to acquire the lock on the parent since the backprop routine needs the children
        // lists to exist, and they won't if we're still creating them
        let parent_index = self.states.index(&parent_state);
        let mut parent = self.states.get_raw_mut(parent_index).unwrap();

        {
            puffin::profile_scope!("create nodes");
            for speculation_piece in EnumSet::all() {
                let evals = next_layer.kind.create_nodes(
                    &children[speculation_piece],
                    parent_index,
                    speculation_piece,
                );
                for (child, eval) in children[speculation_piece].iter().zip(evals.into_iter()) {
                    childs_data.push(Child {
                        mv: child.mv,
                        cached_eval: eval + child.reward,
                        reward: child.reward,
                    });
                }
                childs_indices[speculation_piece as usize + 1] = childs_data.len() as u16;
            }
        }

        let mut children = PackedChildren {
            data: herd.get().alloc_slice_copy(&childs_data),
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

        for &(grandparent, mv, speculation_piece) in parent.parents {
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
        puffin::profile_function!();
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

                    for &(parent, mv, speculation_piece) in parent.parents {
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

pub(super) struct PackedChildren<'bump, E: Evaluation> {
    data: &'bump mut [Child<E>],
    start_indices: [u16; 8],
}

impl<'bump, E: Evaluation> Index<Piece> for PackedChildren<'bump, E> {
    type Output = [Child<E>];

    fn index(&self, index: Piece) -> &Self::Output {
        let start = self.start_indices[index as usize] as usize;
        let end = self.start_indices[index as usize + 1] as usize;
        &self.data[start..end]
    }
}

impl<'bump, E: Evaluation> IndexMut<Piece> for PackedChildren<'bump, E> {
    fn index_mut(&mut self, index: Piece) -> &mut Self::Output {
        let start = self.start_indices[index as usize] as usize;
        let end = self.start_indices[index as usize + 1] as usize;
        &mut self.data[start..end]
    }
}

impl<'bump, E: Evaluation> PackedChildren<'bump, E> {
    pub(super) fn into_children(self, piece: Piece) -> &'bump mut [Child<E>] {
        let start = self.start_indices[piece as usize] as usize;
        let end = self.start_indices[piece as usize + 1] as usize;
        &mut self.data[start..end]
    }
}
