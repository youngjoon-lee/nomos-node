#![allow(dead_code)]
// TODO: Well, remove this when we actually use the fields from the specification
// std

use std::collections::HashSet;
// crates
use futures::{Stream, StreamExt};
// internal
use crate::network::messages::VoteMsg;
use consensus_engine::{Qc, StandardQc, View, Vote};
use nomos_core::crypto::PublicKey;
use nomos_core::vote::Tally;

pub type NodeId = PublicKey;

#[derive(thiserror::Error, Debug)]
pub enum CarnotTallyError {
    #[error("Received invalid vote: {0}")]
    InvalidVote(String),
    #[error("Did not receive enough votes")]
    InsufficientVotes,
}

#[derive(Clone)]
pub struct CarnotTallySettings {
    threshold: usize,
    // TODO: this probably should be dynamic and should change with the view (?)
    participating_nodes: HashSet<NodeId>,
}

#[derive(Clone)]
pub struct CarnotTally {
    settings: CarnotTallySettings,
}

#[async_trait::async_trait]
impl Tally for CarnotTally {
    type Vote = VoteMsg;
    type Qc = Qc;
    type Outcome = HashSet<Vote>;
    type TallyError = CarnotTallyError;
    type Settings = CarnotTallySettings;

    fn new(settings: Self::Settings) -> Self {
        Self { settings }
    }

    async fn tally<S: Stream<Item = Self::Vote> + Unpin + Send>(
        &self,
        view: View,
        mut vote_stream: S,
    ) -> Result<(Self::Qc, Self::Outcome), Self::TallyError> {
        let mut approved = 0usize;
        let mut seen = HashSet::new();
        let mut outcome = HashSet::new();
        while let Some(vote) = vote_stream.next().await {
            // check vote view is valid
            if !vote.vote.view != view {
                return Err(CarnotTallyError::InvalidVote("Invalid view".to_string()));
            }
            // check for duplicated votes
            if seen.contains(&vote.voter) {
                return Err(CarnotTallyError::InvalidVote(
                    "Double voted node".to_string(),
                ));
            }
            // check for individual nodes votes
            if !self.settings.participating_nodes.contains(&vote.voter) {
                return Err(CarnotTallyError::InvalidVote(
                    "Non-participating node".to_string(),
                ));
            }
            seen.insert(vote.voter);
            outcome.insert(vote.vote.clone());
            approved += 1;
            if approved >= self.settings.threshold {
                return Ok((
                    Qc::Standard(StandardQc {
                        view: vote.vote.view,
                        id: vote.vote.block,
                    }),
                    outcome,
                ));
            }
        }
        Err(CarnotTallyError::InsufficientVotes)
    }
}
