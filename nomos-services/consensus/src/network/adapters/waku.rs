// std
use std::borrow::Cow;
// crates
use bytes::Bytes;
use futures::{Stream, StreamExt};
use tokio_stream::wrappers::BroadcastStream;
// internal
use crate::network::{
    messages::{ApprovalMsg, ProposalChunkMsg},
    NetworkAdapter,
};
use crate::overlay::committees::Committee;
use crate::{Approval, View};
use nomos_network::{
    backends::waku::{EventKind, NetworkEvent, Waku, WakuBackendMessage},
    NetworkMsg, NetworkService,
};
use overwatch_rs::services::{relay::OutboundRelay, ServiceData};
use waku_bindings::{Encoding, WakuContentTopic, WakuMessage, WakuPubSubTopic};

pub const WAKU_CARNOT_PUB_SUB_TOPIC: WakuPubSubTopic =
    WakuPubSubTopic::new("CarnotSim", Encoding::Proto);

const APPLICATION_NAME: &str = "CarnotSim";
const VERSION: usize = 1;

pub struct WakuAdapter {
    network_relay: OutboundRelay<<NetworkService<Waku> as ServiceData>::Message>,
}

impl WakuAdapter {
    async fn message_subscriber_channel(
        &self,
    ) -> Result<
        tokio::sync::broadcast::Receiver<NetworkEvent>,
        tokio::sync::oneshot::error::RecvError,
    > {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender,
            })
            .await
        {
            todo!("log error");
        };
        receiver.await
    }

    async fn archive_subscriber_stream(
        &self,
        content_topic: WakuContentTopic,
    ) -> Result<
        Box<dyn Stream<Item = WakuMessage> + Send + Sync + Unpin>,
        tokio::sync::oneshot::error::RecvError,
    > {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::ArchiveSubscribe {
                content_topic,
                reply_channel: sender,
            }))
            .await
        {
            todo!("log error");
        };
        receiver.await
    }

    async fn cached_stream_with_content_topic(
        &self,
        content_topic: WakuContentTopic,
    ) -> impl Stream<Item = WakuMessage> {
        let live_stream_channel = self
            .message_subscriber_channel()
            .await
            .unwrap_or_else(|_e| todo!("handle error"));
        let archive_stream = self
            .archive_subscriber_stream(content_topic.clone())
            .await
            .unwrap();
        let live_stream = BroadcastStream::new(live_stream_channel)
            .zip(futures::stream::repeat(content_topic))
            .filter_map(|(msg, content_topic)| async move {
                match msg {
                    Ok(NetworkEvent::RawMessage(message))
                        if message.content_topic() == &content_topic =>
                    {
                        Some(message)
                    }
                    _ => None,
                }
            });
        tokio_stream::StreamExt::merge(archive_stream, live_stream)
    }
}

#[async_trait::async_trait]
impl NetworkAdapter for WakuAdapter {
    type Backend = Waku;

    async fn new(
        network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
    ) -> Self {
        Self { network_relay }
    }

    async fn proposal_chunks_stream(
        &self,
        committee: Committee,
        view: &View,
    ) -> Box<dyn Stream<Item = Bytes> + Send + Sync + Unpin> {
        let content_topic = proposal_topic(committee, view);
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .map(|message| {
                    let payload = message.payload();
                    ProposalChunkMsg::from_bytes(payload).chunk
                }),
        ))
    }

    async fn broadcast_block_chunk(
        &self,
        committee: Committee,
        view: &View,
        chunk_message: ProposalChunkMsg,
    ) {
        let content_topic = proposal_topic(committee, view);

        let message = WakuMessage::new(
            chunk_message.as_bytes(),
            content_topic,
            1,
            chrono::Utc::now().timestamp() as usize,
        );
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message,
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
            }))
            .await
        {
            todo!("log error");
        };
    }

    async fn approvals_stream(
        &self,
        committee: Committee,
        view: &View,
    ) -> Box<dyn Stream<Item = Approval> + Send> {
        let content_topic = proposal_topic(committee, view);
        Box::new(Box::pin(
            self.cached_stream_with_content_topic(content_topic)
                .await
                .map(|message| {
                    let payload = message.payload();
                    ApprovalMsg::from_bytes(payload).approval
                }),
        ))
    }

    async fn forward_approval(
        &self,
        committee: Committee,
        view: &View,
        approval_message: ApprovalMsg,
    ) {
        let content_topic = approval_topic(committee, view);

        let message = WakuMessage::new(
            approval_message.as_bytes(),
            content_topic,
            1,
            chrono::Utc::now().timestamp() as usize,
        );
        if let Err((_, _e)) = self
            .network_relay
            .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
                message,
                topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
            }))
            .await
        {
            todo!("log error");
        };
    }
}

fn approval_topic(committee: Committee, view: &View) -> WakuContentTopic {
    WakuContentTopic {
        application_name: Cow::Borrowed(APPLICATION_NAME),
        version: VERSION,
        content_topic_name: Cow::Owned(format!("approval-{}-{}", committee.id(), view.id())),
        encoding: Encoding::Proto,
    }
}

fn proposal_topic(committee: Committee, view: &View) -> WakuContentTopic {
    WakuContentTopic {
        application_name: Cow::Borrowed(APPLICATION_NAME),
        version: VERSION,
        content_topic_name: Cow::Owned(format!("proposal-{}-{}", committee.id(), view.id())),
        encoding: Encoding::Proto,
    }
}
