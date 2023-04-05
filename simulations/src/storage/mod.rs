use crate::node::Node;

/// A in-memroy cache stores all of the view number and the vector of nodeids and state
pub(crate) mod full_track;
pub use full_track::FullTrackCache;

/// A in-memory cache stores only the view number and the vector of nodeids and state (no old state)
pub(crate) mod latest_track;

pub trait StateCache<S> {
    fn new<N: Node<State = S>>(nodes: &[N]) -> Self;

    // fn get(&self, view: usize) -> Option<&CachedState<S>>;

    fn update_many<N: Node<State = S>>(&mut self, nodes: &[N]);

    fn update<N: Node<State = S>>(&mut self, node: &N);
}
