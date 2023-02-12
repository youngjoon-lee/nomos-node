use crate::network::messages::*;
use crate::overlay::committees::*;
use crate::overlay::*;
use crate::*;
use async_trait::async_trait;
use bytes::Bytes;
use futures::Stream;
use nomos_core::block::*;
use nomos_core::fountain::{mock::MockFountain, FountainCode, FountainError};
use nomos_network::backends::NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::services::relay::*;
use tokio::sync::broadcast::Receiver;

struct DummyOverlay;
struct DummyAdapter;
struct DummyBackend;

#[async_trait]
impl<'view, N: NetworkAdapter + Sync, F: FountainCode + Sync> Overlay<'view, N, F>
    for DummyOverlay
{
    fn new(_: &View, _: NodeId) -> Self {
        DummyOverlay
    }

    async fn build_qc(&self, _: &N) -> Approval {
        Approval
    }

    async fn broadcast_block(&self, _: Block, _: &N, _: &F) {}

    async fn reconstruct_proposal_block(&self, _: &N, _: &F) -> Result<Block, FountainError> {
        Ok(Block)
    }

    async fn approve_and_forward(&self, _: &Block, _: &N, _: &View) -> Result<(), Box<dyn Error>> {
        Ok(())
    }
}

#[async_trait]
impl NetworkAdapter for DummyAdapter {
    type Backend = DummyBackend;
    async fn new(
        _: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        DummyAdapter
    }
    async fn proposal_chunks_stream(
        &self,
        _: Committee,
        _: &View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        unimplemented!()
    }
    async fn broadcast_block_chunk(&self, _: Committee, _: &View, _: ProposalChunkMsg) {
        unimplemented!()
    }
    async fn approvals_stream(
        &self,
        _: Committee,
        _: &View,
    ) -> Box<dyn Stream<Item = Approval> + Send> {
        unimplemented!()
    }
    async fn forward_approval(&self, _: Committee, _: &View, _: ApprovalMsg) {}
}

#[async_trait]
impl NetworkBackend for DummyBackend {
    type Settings = ();
    type State = NoState<()>;
    type Message = ();
    type EventKind = ();
    type NetworkEvent = ();

    fn new(_config: Self::Settings) -> Self {
        Self
    }
    async fn process(&self, _: Self::Message) {}
    async fn subscribe(&mut self, _: Self::EventKind) -> Receiver<Self::NetworkEvent> {
        unimplemented!()
    }
}

#[tokio::test]
async fn test_single_round_non_leader() {
    let view = View {
        seed: [0; 32],
        staking_keys: BTreeMap::new(),
        view_n: 0,
    };
    let (_, next_view) = view
        .resolve_non_leader::<DummyAdapter, DummyOverlay, MockFountain>(
            [0; 32],
            &DummyAdapter,
            &MockFountain,
        )
        .await
        .unwrap();
    assert!(next_view.view_n == 1);
}