use std::collections::{HashMap, HashSet};
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use rand::seq::SliceRandom;
use sha1::Digest;
use tokio::sync::mpsc;

use crate::core::bitfield::Bitfield;
use crate::core::error::{BError, Result};
use crate::core::metainfo::Metainfo;
use crate::core::net_util::detect_local_ip;
use crate::core::types::{InfoHash, PeerAddr, PeerId, BLOCK_LEN};
use crate::dht::DhtClient;
use crate::peer::connection::PeerContext;
use crate::peer::connection_manager::{
    ConnectionManager, ConnectionManagerConfig,
};
use crate::peer::manager::{PeerEvent};
use crate::peer::message::Message;

#[derive(Clone, Debug)]
pub struct SessionConfig {
    pub dht_endpoint: String,
    pub bind_addr: IpAddr,
    pub peer_port: u16,
    pub max_peers: usize,
    pub pipeline_depth: usize,
    pub dht_refresh_interval_secs: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            dht_endpoint: "http://127.0.0.1:50051".into(),
            bind_addr: detect_local_ip(),
            peer_port: 6881,
            max_peers: 50,
            pipeline_depth: 5,
            dht_refresh_interval_secs: 300,
        }
    }
}

enum PieceState {
    Pending,
    Downloading {
        blocks: Vec<Option<Vec<u8>>>,
        received: usize,
    },
    Complete(Vec<u8>),
}

struct PieceTracker {
    pieces: Vec<PieceState>,
    metainfo: Metainfo,
    piece_availability: Vec<usize>,
    inflight: HashMap<(PeerId, u32), HashSet<u32>>,
    peer_pipeline: HashMap<PeerId, usize>,
}

impl PieceTracker {
    fn new(metainfo: Metainfo) -> Self {
        let count = metainfo.piece_count();
        Self {
            pieces: (0..count).map(|_| PieceState::Pending).collect(),
            metainfo,
            piece_availability: vec![0; count],
            inflight: HashMap::new(),
            peer_pipeline: HashMap::new(),
        }
    }

    fn is_complete(&self) -> bool {
        self.pieces
            .iter()
            .all(|s| matches!(s, PieceState::Complete(_)))
    }

    fn completed_count(&self) -> usize {
        self.pieces
            .iter()
            .filter(|s| matches!(s, PieceState::Complete(_)))
            .count()
    }

    #[allow(dead_code)]
    fn has_piece(&self, index: usize) -> bool {
        matches!(self.pieces.get(index), Some(PieceState::Complete(_)))
    }

    fn increase_availability(&mut self, indexes: impl Iterator<Item = usize>) {
        for i in indexes {
            if i < self.piece_availability.len() {
                self.piece_availability[i] = self.piece_availability[i].saturating_add(1);
            }
        }
    }

    fn decrease_availability(&mut self, indexes: impl Iterator<Item = usize>) {
        for i in indexes {
            if i < self.piece_availability.len() {
                self.piece_availability[i] = self.piece_availability[i].saturating_sub(1);
            }
        }
    }

    fn pick_rarest(&self, peer_bitfield: &Bitfield) -> Option<u32> {
        let mut candidates: Vec<(u32, usize)> = peer_bitfield
            .complete_pieces()
            .filter(|&i| matches!(self.pieces[i], PieceState::Pending))
            .map(|i| (i as u32, self.piece_availability[i]))
            .collect();

        candidates.sort_by_key(|(_, avail)| *avail);

        let min_avail = candidates.first()?.1;
        let rarest: Vec<u32> = candidates
            .iter()
            .take_while(|(_, a)| *a == min_avail)
            .map(|(i, _)| *i)
            .collect();

        rarest.choose(&mut rand::thread_rng()).copied()
    }

    fn pick_next_block(&self, piece_index: u32) -> Option<u32> {
        match &self.pieces[piece_index as usize] {
            PieceState::Pending => Some(0),
            PieceState::Downloading { blocks, .. } => blocks
                .iter()
                .position(|b| b.is_none())
                .map(|i| i as u32 * BLOCK_LEN),
            PieceState::Complete(_) => None,
        }
    }

    fn ensure_downloading(&mut self, piece_index: u32) {
        if matches!(self.pieces[piece_index as usize], PieceState::Pending) {
            let piece_len = self.metainfo.piece_length_for(piece_index as usize);
            let block_count = ((piece_len as u32 + BLOCK_LEN - 1) / BLOCK_LEN) as usize;
            self.pieces[piece_index as usize] = PieceState::Downloading {
                blocks: vec![None; block_count],
                received: 0,
            };
        }
    }

    fn store_block(&mut self, index: u32, begin: u32, data: Vec<u8>) -> bool {
        self.ensure_downloading(index);

        if let PieceState::Downloading { blocks, received } =
            &mut self.pieces[index as usize]
        {
            let block_idx = (begin / BLOCK_LEN) as usize;
            if block_idx < blocks.len() && blocks[block_idx].is_none() {
                blocks[block_idx] = Some(data);
                *received += 1;
                return *received == blocks.len();
            }
        }
        false
    }

    fn verify_and_complete(&mut self, index: u32) -> Result<Vec<u8>> {
        let data = self.assemble(index);
        let expected = self.metainfo.piece_hash(index as usize).ok_or_else(|| {
            BError::Session(format!("no hash for piece {}", index))
        })?;

        let actual: [u8; 20] = sha1::Sha1::digest(&data).into();
        if actual != *expected {
            self.reset(index);
            return Err(BError::Session(format!(
                "SHA1 mismatch for piece {}",
                index
            )));
        }

        self.pieces[index as usize] = PieceState::Complete(data.clone());
        Ok(data)
    }

    fn reset(&mut self, index: u32) {
        self.pieces[index as usize] = PieceState::Pending;
    }

    fn assemble(&self, index: u32) -> Vec<u8> {
        match &self.pieces[index as usize] {
            PieceState::Downloading { blocks, .. } => {
                let piece_len = self.metainfo.piece_length_for(index as usize);
                let mut data = Vec::with_capacity(piece_len);
                for block in blocks.iter().flatten() {
                    data.extend_from_slice(block);
                }
                data
            }
            PieceState::Complete(data) => data.clone(),
            _ => Vec::new(),
        }
    }

    fn take_data(&mut self) -> Vec<u8> {
        let total = self.metainfo.info.total_length;
        let mut data = Vec::with_capacity(total);
        for piece in self.pieces.iter_mut() {
            if let PieceState::Complete(d) = piece {
                data.extend_from_slice(d);
            }
        }
        data.truncate(total);
        data
    }

    fn mark_inflight(&mut self, peer_id: PeerId, index: u32, begin: u32) {
        self.inflight
            .entry((peer_id, index))
            .or_default()
            .insert(begin);
        *self.peer_pipeline.entry(peer_id).or_default() += 1;
    }

    fn clear_inflight(&mut self, peer_id: PeerId, index: u32, begin: u32) {
        if let Some(set) = self.inflight.get_mut(&(peer_id, index)) {
            set.remove(&begin);
            if set.is_empty() {
                self.inflight.remove(&(peer_id, index));
            }
        }
        if let Some(c) = self.peer_pipeline.get_mut(&peer_id) {
            *c = c.saturating_sub(1);
        }
    }

    fn cancel_all_inflight(&mut self, peer_id: &PeerId) {
        self.inflight.retain(|(pid, _), _| pid != peer_id);
        self.peer_pipeline.remove(peer_id);
    }

    fn peer_pipeline_count(&self, peer_id: &PeerId) -> usize {
        self.peer_pipeline.get(peer_id).copied().unwrap_or(0)
    }
}

pub struct Session {
    dht: DhtClient,
    cm: ConnectionManager,
    metainfo: Metainfo,
    #[allow(dead_code)]
    our_peer_id: PeerId,
    our_bitfield: Bitfield,
    piece_tracker: PieceTracker,
    peer_contexts: HashMap<PeerId, PeerContext>,
    event_rx: mpsc::Receiver<PeerEvent>,
    #[allow(dead_code)]
    event_tx: mpsc::Sender<PeerEvent>,
    config: SessionConfig,
    upload_bytes: HashMap<PeerId, u64>,
    tft_round: u64,
}

impl Session {
    pub async fn new(
        config: SessionConfig,
        metainfo: Metainfo,
    ) -> Result<Self> {
        let dht = DhtClient::connect(&config.dht_endpoint).await?;
        let (event_tx, event_rx) = mpsc::channel(1024);
        let our_peer_id = PeerId::new_random();
        let our_bitfield = Bitfield::new(metainfo.piece_count());

        let cm = ConnectionManager::new(
            ConnectionManagerConfig {
                max_peers: config.max_peers,
                connect_timeout_secs: 10,
            },
            event_tx.clone(),
            our_peer_id,
        );

        let piece_tracker = PieceTracker::new(metainfo.clone());

        Ok(Self {
            dht,
            cm,
            metainfo,
            our_peer_id,
            our_bitfield,
            piece_tracker,
            peer_contexts: HashMap::new(),
            event_rx,
            event_tx,
            config,
            upload_bytes: HashMap::new(),
            tft_round: 0,
        })
    }

    pub fn progress(&self) -> f64 {
        let total = self.metainfo.piece_count();
        if total == 0 {
            return 1.0;
        }
        self.piece_tracker.completed_count() as f64 / total as f64
    }

    pub fn info_hash(&self) -> &InfoHash {
        &self.metainfo.info_hash
    }

    pub fn metainfo(&self) -> &Metainfo {
        &self.metainfo
    }

    pub async fn download(&mut self) -> Result<Vec<u8>> {
        let our_addr = PeerAddr::new(self.config.bind_addr, self.config.peer_port);

        self.dht
            .announce_peer(&self.metainfo.info_hash, &our_addr)
            .await?;

        let peers = self
            .dht
            .get_peers(&self.metainfo.info_hash)
            .await?;

        let our_bitfield = Arc::new(std::sync::Mutex::new(self.our_bitfield.clone()));

        self.cm
            .start_listener(
                self.config.peer_port,
                self.metainfo.info_hash,
                our_bitfield,
            )
            .await?;

        if !peers.is_empty() {
            self.cm
                .connect_to_peers(
                    &peers,
                    self.metainfo.info_hash,
                    &self.our_bitfield,
                )
                .await?;
        }

        self.run_event_loop().await
    }

    async fn run_event_loop(&mut self) -> Result<Vec<u8>> {
        let mut tft_timer =
            tokio::time::interval(Duration::from_secs(10));
        let mut dht_timer = tokio::time::interval(Duration::from_secs(
            self.config.dht_refresh_interval_secs,
        ));

        loop {
            self.cm.drain_new_handles();

            tokio::select! {
                event = self.event_rx.recv() => {
                    match event {
                        Some(PeerEvent::HandshakeComplete(ctx)) => {
                            let peer_id = ctx.peer_id;
                            if !self.peer_contexts.contains_key(&peer_id) {
                                self.peer_contexts.insert(peer_id, ctx);
                            }
                        }
                        Some(PeerEvent::ReceivedMessage { peer_id, msg }) => {
                            self.handle_peer_message(peer_id, msg).await;
                        }
                        Some(PeerEvent::Disconnected(addr)) => {
                            self.handle_disconnected(addr).await;
                        }
                        None => break,
                    }
                }

                _ = tft_timer.tick() => {
                    self.run_tit_for_tat().await;
                }

                _ = dht_timer.tick() => {
                    if let Err(e) = self.refresh_peers().await {
                        tracing::warn!("DHT refresh failed: {}", e);
                    }
                }

                else => {
                    if self.piece_tracker.is_complete() {
                        break;
                    }
                }
            }

            if self.piece_tracker.is_complete() {
                break;
            }
        }

        self.cm.disconnect_all().await;
        Ok(self.piece_tracker.take_data())
    }

    async fn handle_peer_message(
        &mut self,
        peer_id: PeerId,
        msg: crate::peer::message::Message,
    ) {
        use crate::peer::message::Message;

        match msg {
            Message::Choke => {
                if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
                    ctx.peer_choking = true;
                }
                self.piece_tracker.cancel_all_inflight(&peer_id);
            }

            Message::Unchoke => {
                if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
                    ctx.peer_choking = false;
                }
                self.fill_pipeline(peer_id).await;
            }

            Message::Interested => {
                if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
                    ctx.peer_interested = true;
                }
            }

            Message::NotInterested => {
                if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
                    ctx.peer_interested = false;
                }
            }

            Message::Have(index) => {
                if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
                    if !ctx.peer_bitfield.has(index as usize) {
                        ctx.peer_bitfield.set(index as usize);
                        self.piece_tracker
                            .increase_availability(std::iter::once(index as usize));
                        ctx.update_interest(&self.our_bitfield);

                        if ctx.am_interested
                            && !ctx.peer_choking
                            && self.piece_tracker.peer_pipeline_count(&peer_id)
                                < self.config.pipeline_depth
                        {
                            self.fill_pipeline(peer_id).await;
                        }
                    }
                }
            }

            Message::Bitfield(bf) => {
                let prev: Vec<usize> = self
                    .peer_contexts
                    .get(&peer_id)
                    .map(|ctx| ctx.peer_bitfield.complete_pieces().collect())
                    .unwrap_or_default();

                self.piece_tracker
                    .decrease_availability(prev.into_iter());

                if let Some(ctx) = self.peer_contexts.get_mut(&peer_id) {
                    let new: Vec<usize> = bf.complete_pieces().collect();
                    self.piece_tracker.increase_availability(new.into_iter());
                    ctx.peer_bitfield = bf;
                    ctx.update_interest(&self.our_bitfield);

                    if ctx.am_interested {
                        let _ = self
                            .cm
                            .send_to(&peer_id, Message::Interested)
                            .await;
                        let _ = self
                            .cm
                            .send_to(&peer_id, Message::Unchoke)
                            .await;
                    }
                }
            }

            Message::Request { index, begin, length } => {
                if let Some(ctx) = self.peer_contexts.get(&peer_id) {
                    if !ctx.am_choking && self.our_bitfield.has(index as usize) {
                        let data = self.piece_tracker.assemble(index);
                        let offset = begin as usize;
                        let end = (offset + length as usize).min(data.len());
                        let block = data[offset..end].to_vec();

                        let _ = self
                            .cm
                            .send_to(
                                &peer_id,
                                Message::Piece {
                                    index,
                                    begin,
                                    data: block,
                                },
                            )
                            .await;

                        *self.upload_bytes.entry(peer_id).or_default() +=
                            length as u64;
                    }
                }
            }

            Message::Piece { index, begin, data } => {
                self.piece_tracker
                    .clear_inflight(peer_id, index, begin);

                let is_complete = self.piece_tracker.store_block(index, begin, data);

                if is_complete {
                    match self.piece_tracker.verify_and_complete(index) {
                        Ok(_data) => {
                            self.our_bitfield.set(index as usize);
                            let _ = self
                                .cm
                                .broadcast(Message::Have(index))
                                .await;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "piece {} verification failed: {}",
                                index,
                                e
                            );
                        }
                    }
                }

                if self.piece_tracker.peer_pipeline_count(&peer_id)
                    < self.config.pipeline_depth
                {
                    self.fill_pipeline(peer_id).await;
                }
            }

            Message::Cancel { index, begin, .. } => {
                self.piece_tracker.clear_inflight(peer_id, index, begin);
            }
        }
    }

    async fn fill_pipeline(&mut self, peer_id: PeerId) {
        let peer_bf = match self.peer_contexts.get(&peer_id) {
            Some(ctx) if !ctx.peer_choking && ctx.am_interested => {
                ctx.peer_bitfield.clone()
            }
            _ => return,
        };

        while self.piece_tracker.peer_pipeline_count(&peer_id)
            < self.config.pipeline_depth
        {
            let piece_index = match self.piece_tracker.pick_rarest(&peer_bf) {
                Some(i) => i,
                None => break,
            };

            let begin = match self.piece_tracker.pick_next_block(piece_index) {
                Some(b) => b,
                None => break,
            };

            let length = self
                .metainfo
                .block_length_for(piece_index as usize, begin);

            let _ = self
                .cm
                .send_to(
                    &peer_id,
                    crate::peer::message::Message::Request {
                        index: piece_index,
                        begin,
                        length,
                    },
                )
                .await;

            self.piece_tracker.mark_inflight(peer_id, piece_index, begin);
        }
    }

    async fn handle_disconnected(&mut self, addr: PeerAddr) {
        let disconnected_id: Option<PeerId> = self
            .cm
            .peers()
            .find(|(_, h)| h.addr == addr)
            .map(|(id, _)| *id);

        let Some(peer_id) = disconnected_id else {
            return;
        };

        let old_bf: Vec<usize> = self
            .peer_contexts
            .get(&peer_id)
            .map(|ctx| ctx.peer_bitfield.complete_pieces().collect())
            .unwrap_or_default();

        self.piece_tracker.decrease_availability(old_bf.into_iter());
        self.piece_tracker.cancel_all_inflight(&peer_id);
        self.peer_contexts.remove(&peer_id);
        self.upload_bytes.remove(&peer_id);
        self.cm.remove_disconnected(&peer_id);
    }

    async fn run_tit_for_tat(&mut self) {
        self.tft_round += 1;

        let mut interested: Vec<PeerId> = self
            .peer_contexts
            .iter()
            .filter(|(_, ctx)| ctx.peer_interested)
            .map(|(id, _)| *id)
            .collect();

        interested.sort_by_key(|id| {
            std::cmp::Reverse(
                self.upload_bytes.get(id).copied().unwrap_or(0),
            )
        });

        for &id in interested.iter().take(4) {
            if let Some(ctx) = self.peer_contexts.get_mut(&id) {
                if ctx.am_choking {
                    ctx.am_choking = false;
                    let _ = self.cm.send_to(&id, Message::Unchoke).await;
                }
            }
        }

        for &id in interested.iter().skip(4) {
            if let Some(ctx) = self.peer_contexts.get_mut(&id) {
                if !ctx.am_choking {
                    ctx.am_choking = true;
                    let _ = self.cm.send_to(&id, Message::Choke).await;
                }
            }
        }

        if self.tft_round % 3 == 0 {
            let choked: Vec<PeerId> = self
                .peer_contexts
                .iter()
                .filter(|(_, ctx)| {
                    ctx.peer_interested && ctx.am_choking
                })
                .map(|(id, _)| *id)
                .collect();

            if let Some(lucky) = choked.choose(&mut rand::thread_rng()) {
                if let Some(ctx) = self.peer_contexts.get_mut(lucky) {
                    ctx.am_choking = false;
                    let _ = self.cm.send_to(lucky, Message::Unchoke).await;
                }
            }
        }
    }

    async fn refresh_peers(&mut self) -> Result<()> {
        let our_addr = PeerAddr::new(self.config.bind_addr, self.config.peer_port);

        self.dht
            .announce_peer(&self.metainfo.info_hash, &our_addr)
            .await?;

        let peers = self.dht.get_peers(&self.metainfo.info_hash).await?;

        if !peers.is_empty() {
            self.cm
                .connect_to_peers(
                    &peers,
                    self.metainfo.info_hash,
                    &self.our_bitfield,
                )
                .await?;
        }

        Ok(())
    }
}
