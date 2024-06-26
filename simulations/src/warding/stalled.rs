use crate::runner::BoxedNode;
use crate::warding::{SimulationState, SimulationWard};
use serde::{Deserialize, Serialize};

/// StalledView. Track stalled nodes (e.g incoming queue is empty, the node doesn't write to other queues)
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StalledViewWard {
    // the hash checksum
    consecutive_viewed_checkpoint: Option<u32>,
    // use to check if the node is stalled
    criterion: usize,
    threshold: usize,
}

impl StalledViewWard {
    fn update_state(&mut self, cks: u32) {
        match &mut self.consecutive_viewed_checkpoint {
            Some(cp) => {
                if cks == *cp {
                    self.criterion += 1;
                } else {
                    *cp = cks;
                    // reset the criterion
                    self.criterion = 0;
                }
            }
            None => {
                self.consecutive_viewed_checkpoint = Some(cks);
            }
        }
    }
}

impl<S, T> SimulationWard<S, T> for StalledViewWard {
    type SimulationState = SimulationState<S, T>;
    fn analyze(&mut self, state: &Self::SimulationState) -> bool {
        let nodes = state.nodes.read();
        self.update_state(checksum(nodes.as_slice()));
        self.criterion >= self.threshold
    }
}

#[inline]
fn checksum<S, T>(nodes: &[BoxedNode<S, T>]) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    for node in nodes.iter() {
        let view: i64 = node.current_view().into();
        hash.update(&(view as usize).to_be_bytes());
        // TODO: hash messages in the node
    }

    hash.finalize()
}

#[cfg(test)]
mod test {
    use super::*;
    use parking_lot::RwLock;
    use std::sync::Arc;

    #[test]
    fn rebase_threshold() {
        let mut stalled = StalledViewWard {
            consecutive_viewed_checkpoint: None,
            criterion: 0,
            threshold: 2,
        };
        let state = SimulationState {
            nodes: Arc::new(RwLock::new(vec![Box::new(10)])),
        };

        // increase the criterion, 1
        assert!(!stalled.analyze(&state));
        // increase the criterion, 2
        assert!(!stalled.analyze(&state));
        // increase the criterion, 3 > threshold 2, so true
        assert!(stalled.analyze(&state));

        // push a new one, so the criterion is reset to 0
        state.nodes.write().push(Box::new(20));
        assert!(!stalled.analyze(&state));

        // increase the criterion, 2
        assert!(!stalled.analyze(&state));
        // increase the criterion, 3 > threshold 2, so true
        assert!(stalled.analyze(&state));
    }
}
