//! # Layered simulation runner
//!
//! A revision of the [`glauber`](super::glauber_runner) simulation runner.
//!
//! **`glauber`** simulations have some drawbacks:
//!
//! * Completely random, difficult to control
//! * Not close to how real nodes would perform in reality
//! * Difficult to analise recorded data, as data it is updated by chunks of iterations
//!
//! To solve this we can use a concept of layered *glauber* executions.
//! The algorithm roughly works as follows:
//!
//! ```python
//! nodes <- [nodes]
//! layers <- [[nodes_ids], [], ...]
//! while nodes_to_compute(layers):
//!     layer_index <- pick_rand_layer(layers)
//!     node_index <- pop_rand_node(rand_layer)
//!     step(nodes[node_index])
//!     if not node_decided(node):
//!         push(layers[layer_index+1], node_index)
//! ```
//!
//! From within this, controlling the *number of layers*, and *weighting* them (how often are they picked),
//! we can control the flow of the simulations.
//! Also we can consider that once the bottom layer is empty a fully step have been concluded and we can record
//! the data of that step simulation.

// std
use crossbeam::channel::bounded;
use crossbeam::select;
use std::collections::BTreeSet;
use std::ops::Not;
use std::sync::Arc;
// crates
use fixed_slice_deque::FixedSliceDeque;
use rand::prelude::{IteratorRandom, SliceRandom};
use rand::rngs::SmallRng;
use serde::Serialize;
// internal
use crate::node::{Node, NodeId};
use crate::overlay::Overlay;
use crate::runner::SimulationRunner;
use crate::streaming::{Producer, Subscriber};
use crate::warding::SimulationState;

use super::SimulationRunnerHandle;

/// Simulate with sending the network state to any subscriber
pub fn simulate<M, N: Node, O: Overlay, P: Producer>(
    runner: SimulationRunner<M, N, O, P>,
    gap: usize,
    distribution: Option<Vec<f32>>,
) -> anyhow::Result<SimulationRunnerHandle>
where
    M: Send + Sync + Clone + 'static,
    N: Send + Sync + 'static,
    N::Settings: Clone + Send,
    N::State: Serialize,
    O::Settings: Clone + Send,
    P::Subscriber: Send + Sync + 'static,
    <P::Subscriber as Subscriber>::Record:
        for<'a> TryFrom<&'a SimulationState<N>, Error = anyhow::Error>,
{
    let distribution =
        distribution.unwrap_or_else(|| std::iter::repeat(1.0f32).take(gap).collect());

    let layers: Vec<usize> = (0..gap).collect();

    let mut deque = build_node_ids_deque(gap, &runner);

    let simulation_state = SimulationState {
        nodes: Arc::clone(&runner.nodes),
    };

    let inner = runner.inner.clone();
    let nodes = runner.nodes.clone();
    let (stop_tx, stop_rx) = bounded(1);
    let handle = SimulationRunnerHandle {
        stop_tx,
        handle: std::thread::spawn(move || {
            let p = P::new(runner.stream_settings.settings)?;
            scopeguard::defer!(if let Err(e) = p.stop() {
                eprintln!("Error stopping producer: {e}");
            });
            let sub = p.subscribe()?;
            std::thread::spawn(move || {
                if let Err(e) = sub.run() {
                    eprintln!("Error running subscriber: {e}");
                }
            });
            loop {
                select! {
                    recv(stop_rx) -> _ => {
                        break;
                    }
                    default => {
                        let mut inner = inner.write();
                        let (group_index, node_id) =
                            choose_random_layer_and_node_id(&mut inner.rng, &distribution, &layers, &mut deque);

                        // remove node_id from group
                        deque.get_mut(group_index).unwrap().remove(&node_id);

                        {
                            let mut shared_nodes = nodes.write();
                            let node: &mut N = shared_nodes
                                .get_mut(node_id.inner())
                                .expect("Node should be present");
                            let prev_view = node.current_view();
                            node.step();
                            let after_view = node.current_view();
                            if after_view > prev_view {
                                // pass node to next step group
                                deque.get_mut(group_index + 1).unwrap().insert(node_id);
                            }
                        }

                        // check if any condition makes the simulation stop
                        if inner.check_wards(&simulation_state) {
                            break;
                        }

                        // if initial is empty then we finished a full round, append a new set to the end so we can
                        // compute the most advanced nodes again
                        if deque.first().unwrap().is_empty() {
                            let _ = deque.push_back(BTreeSet::default());
                            p.send(<P::Subscriber as Subscriber>::Record::try_from(
                                &simulation_state,
                            )?)?;
                        }

                        // if no more nodes to compute
                        if deque.iter().all(BTreeSet::is_empty) {
                            break;
                        }
                    }
                }
            }
            // write latest state
            p.send(<P::Subscriber as Subscriber>::Record::try_from(
                &simulation_state,
            )?)?;
            Ok(())
        }),
    };
    Ok(handle)
}

fn choose_random_layer_and_node_id(
    rng: &mut SmallRng,
    distribution: &[f32],
    layers: &[usize],
    deque: &mut FixedSliceDeque<BTreeSet<NodeId>>,
) -> (usize, NodeId) {
    let i = *layers
        .iter()
        // filter out empty round groups
        .filter_map(|&i| {
            let g = deque.get(i).unwrap();
            g.is_empty().not().then_some(i)
        })
        // intermediate collect necessary for choose_weighted
        .collect::<Vec<_>>()
        .choose_weighted(rng, |&i| distribution.get(i).unwrap())
        .expect("Distribution choose to work");

    let group: &mut BTreeSet<NodeId> = deque.get_mut(i).unwrap();

    let node_id = group.iter().choose(rng).unwrap();
    (i, *node_id)
}

fn build_node_ids_deque<M, N, O, P>(
    gap: usize,
    runner: &SimulationRunner<M, N, O, P>,
) -> FixedSliceDeque<BTreeSet<NodeId>>
where
    N: Node,
    O: Overlay,
    P: Producer,
{
    // add a +1 so we always have
    let mut deque = FixedSliceDeque::new(gap + 1);
    // push first layer
    let node_ids: BTreeSet<NodeId> = runner.nodes.write().iter().map(|node| node.id()).collect();

    deque.push_back(node_ids);
    // allocate default sets
    while deque.try_push_back(BTreeSet::new()).is_ok() {}
    deque
}
