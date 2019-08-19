/*******************************************************************************
 * Copyright (c) 2018-2019 Aion foundation.
 *
 *     This file is part of the aion network project.
 *
 *     The aion network project is free software: you can redistribute it
 *     and/or modify it under the terms of the GNU General Public License
 *     as published by the Free Software Foundation, either version 3 of
 *     the License, or any later version.
 *
 *     The aion network project is distributed in the hope that it will
 *     be useful, but WITHOUT ANY WARRANTY; without even the implied
 *     warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
 *     See the GNU General Public License for more details.
 *
 *     You should have received a copy of the GNU General Public License
 *     along with the aion network project source files.
 *     If not, see <https://www.gnu.org/licenses/>.
 *
 ******************************************************************************/

mod event;
mod handler;
mod route;
mod helper;
mod storage;
#[cfg(test)]
mod test;

use std::sync::RwLock;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use std::collections::BTreeMap;
use std::collections::HashMap;
use rustc_hex::ToHex;
use client::BlockChainClient;
use client::BlockId;
use client::BlockStatus;
use client::ChainNotify;
use transaction::UnverifiedTransaction;
use aion_types::{H256,U256};
use futures::Future;
use futures::Stream;
use lru_cache::LruCache;
use rlp::UntrustedRlp;
use tokio::runtime::Runtime;
use tokio::timer::Interval;
use bytes::BufMut;
use byteorder::{BigEndian,ByteOrder};

use p2p::Node;
use p2p::ChannelBuffer;
use p2p::Config;
use p2p::Mgr;
use p2p::send;
use sync::route::VERSION;
use sync::route::MODULE;
use sync::route::ACTION;
use sync::handler::status;
use sync::handler::headers;
// use sync::handler::bodies;
use sync::handler::broadcast;
// use sync::handler::import;
use self::helper::HeadersWrapper;
use sync::storage::ActivePeerInfo;
use sync::storage::PeerInfo;
use sync::storage::SyncState;
use sync::storage::SyncStatus;
use sync::storage::SyncStorage;
use sync::storage::TransactionStats;
use p2p::get_random_active_node_hash;
use p2p::get_random_active_node;

const STATUS_REQ_INTERVAL: u64 = 2;
const BLOCKS_BODIES_REQ_INTERVAL: u64 = 50;
const BLOCKS_IMPORT_INTERVAL: u64 = 50;
const BROADCAST_TRANSACTIONS_INTERVAL: u64 = 50;
const INTERVAL_STATUS: u64 = 10;
const INTERVAL_HEADERS: u64 = 2;
const HEADERS_STEP: u32 = 64;
const MAX_TX_CACHE: usize = 20480;
const MAX_BLOCK_CACHE: usize = 32;

pub struct Sync {
    config: Arc<Config>,
    client: Arc<BlockChainClient>,
    runtime: Arc<Runtime>,
    p2p: Arc<Mgr>,

    /// collection of sent headers
    headers: Arc<RwLock<BTreeMap<u64, HeadersWrapper>>>,

    /// local best td
    local_best_td: Arc<RwLock<U256>>,

    /// local best block number
    local_best_block_number: Arc<RwLock<u64>>,

    /// network best td
    network_best_td: Arc<RwLock<U256>>,

    /// network best block number
    network_best_block_number: Arc<RwLock<u64>>,

    /// cache tx hash which has been stored and broadcasted
    cached_tx_hashes: Arc<Mutex<LruCache<H256, u8>>>,

    /// cache block hash which has been committed and broadcasted
    cached_block_hashes:  Arc<Mutex<LruCache<H256, u8>>>  
}

impl Sync {
    pub fn new(config: Config, client: Arc<BlockChainClient>) -> Sync {
        let local_best_td: U256 = client.chain_info().total_difficulty;
        let local_best_block_number: u64 = client.chain_info().best_block_number;
        let config = Arc::new(config);
        Sync {
            config: config.clone(),
            client,
            p2p: Arc::new(Mgr::new(config)),
            runtime: Arc::new(Runtime::new().expect("tokio runtime")),
            headers: Arc::new(RwLock::new(BTreeMap::new())),

            local_best_td: Arc::new(RwLock::new(local_best_td)),
            local_best_block_number: Arc::new(RwLock::new(local_best_block_number)),
            network_best_td: Arc::new(RwLock::new(local_best_td)),
            network_best_block_number: Arc::new(RwLock::new(local_best_block_number)),
            cached_tx_hashes: Arc::new(Mutex::new(LruCache::new(MAX_TX_CACHE))),
            cached_block_hashes: Arc::new(Mutex::new(LruCache::new(MAX_BLOCK_CACHE))),
        }
    }

    pub fn run(&self) {
        // counters
        let runtime = self.runtime.clone();
        let executor = Arc::new(runtime.executor());
        let nodes = self.p2p.nodes.clone();

        // init p2p
        &self.p2p.run(Arc::new(handle), self.headers.clone());

        // status
        let executor_status = executor.clone();
        let nodes_status = nodes.clone();
        let nodes_headers = nodes.clone();
        let nodes_send_0 = nodes.clone();
        let nodes_send_1 = nodes.clone();

        executor_status.spawn(
            Interval::new(Instant::now(), Duration::from_secs(INTERVAL_STATUS))
                .for_each(move |_| {
                    // make it constant
                    if let Some(hash) = get_random_active_node_hash(nodes_status.clone()) {
                        let mut cb = ChannelBuffer::new();
                        cb.head.ver = VERSION::V0.value();
                        cb.head.ctrl = MODULE::SYNC.value();
                        cb.head.action = ACTION::STATUSREQ.value();
                        cb.head.len = 0;
                        send(hash, cb, nodes_send_0.clone());
                    }

                    // p2p.get_node_by_td(10);
                    Ok(())
                })
                .map_err(|err| error!(target: "p2p", "executor status: {:?}", err)),
        );

        let executor_headers = executor.clone();
        executor_headers.spawn(
            Interval::new(Instant::now(), Duration::from_secs(INTERVAL_HEADERS))
                .for_each(move |_| {
                    // make it constant
                    if let Some(node) = get_random_active_node(nodes_headers.clone()) {
                        let chain_info = SyncStorage::get_chain_info();
                        if node.total_difficulty > chain_info.total_difficulty
                            && node.block_num - HEADERS_STEP as u64 >= chain_info.best_block_number
                        {
                            let start = if chain_info.best_block_number > 3 {
                                chain_info.best_block_number - 3
                            } else {
                                1
                            };

                            let mut cb = ChannelBuffer::new();
                            cb.head.ver = VERSION::V0.value();
                            cb.head.ctrl = MODULE::SYNC.value();
                            cb.head.action = ACTION::HEADERSREQ.value();

                            let mut from_buf = [0u8; 8];
                            BigEndian::write_u64(&mut from_buf, start);
                            cb.body.put_slice(&from_buf);

                            let mut size_buf = [0u8; 4];
                            BigEndian::write_u32(&mut size_buf, HEADERS_STEP);
                            cb.body.put_slice(&size_buf);

                            cb.head.len = cb.body.len() as u32;
                            send(node.get_hash(), cb, nodes_send_1.clone());
                        }
                    }

                    //                     p2p.get_node_by_td(10);
                    Ok(())
                })
                .map_err(|err| error!(target: "p2p", "executor status: {:?}", err)),
        )
    }

    pub fn shutdown(&self) {
        // SyncMgr::disable();
        // TODO: update proper ways to clear up threads and connections on p2p layer
        let p2p = self.p2p.clone();
        p2p.shutdown();
    }
}

pub fn handle(
    hash: u64,
    cb: ChannelBuffer,
    nodes: Arc<RwLock<HashMap<u64, Node>>>,
    hws: Arc<RwLock<BTreeMap<u64, HeadersWrapper>>>,
    local_best_block_number: Arc<RwLock<u64>>,
    network_best_block_number: Arc<RwLock<u64>>,
    cached_tx_hashes: Arc<Mutex<LruCache<H256, u8>>>,
    cached_block_hashes:  Arc<Mutex<LruCache<H256, u8>>>
){
    match ACTION::from(cb.head.action) {
        ACTION::STATUSREQ => {
            if cb.head.len != 0 {
                // TODO: kill the node
            }
            status::receive_req(hash, nodes)
        }
        ACTION::STATUSRES => status::receive_res(hash, cb, nodes),
        ACTION::HEADERSREQ => headers::receive_req(hash, cb, nodes),
        ACTION::HEADERSRES => headers::receive_res(hash, cb, nodes, hws),
        ACTION::BODIESREQ => (),
        ACTION::BODIESRES => (),
        // ACTION::BROADCASTTX => broadcast::receive_tx(hash, cb, nodes, localnetwork_best_block_number, client, cached_tx_hashes),
        ACTION::BROADCASTBLOCK => (), // broadcast::receive_block(hash, cb, nodes),
        ACTION::UNKNOWN => (),
    };
}

pub trait SyncProvider: Send + ::std::marker::Sync {
    /// Get sync status
    fn status(&self) -> SyncStatus;

    /// Get peers information
    fn peers(&self) -> Vec<PeerInfo>;

    /// Get the enode if available.
    fn enode(&self) -> Option<String>;

    /// Returns propagation count for pending transactions.
    fn transactions_stats(&self) -> BTreeMap<H256, TransactionStats>;

    /// Get active nodes
    fn active(&self) -> Vec<ActivePeerInfo>;
}

impl SyncProvider for Sync {
    /// Get sync status
    fn status(&self) -> SyncStatus {
        // TODO:  only set start_block_number/highest_block_number.
        SyncStatus {
            state: SyncState::Idle,
            protocol_version: 0,
            network_id: 256,
            start_block_number: self.client.chain_info().best_block_number,
            last_imported_block_number: None,
            highest_block_number: { Some(SyncStorage::get_network_best_block_number()) },
            blocks_received: 0,
            blocks_total: 0,
            //num_peers: { get_nodes_count(ALIVE.value()) },
            num_peers: 0,
            num_active_peers: 0,
        }
    }

    /// Get sync peers
    fn peers(&self) -> Vec<PeerInfo> {
        // let mut peer_info_list = Vec::new();
        // let peer_nodes = get_all_nodes();
        // for peer in peer_nodes.iter() {
        //     let peer_info = PeerInfo {
        //         id: Some(peer.get_node_id()),
        //     };
        //     peer_info_list.push(peer_info);
        // }
        // peer_info_list
        Vec::new()
    }

    fn enode(&self) -> Option<String> {
        // Some(get_local_node().get_node_id())
        None
    }

    fn transactions_stats(&self) -> BTreeMap<H256, TransactionStats> { BTreeMap::new() }

    fn active(&self) -> Vec<ActivePeerInfo> {
        let nodes = &self.p2p.get_active_nodes();
        nodes
            .into_iter()
            .map(|node| {
                ActivePeerInfo {
                    highest_block_number: node.block_num,
                    id: node.id.to_hex(),
                    ip: node.addr.ip.to_hex(),
                }
            })
            .collect()
    }
}

impl ChainNotify for Sync {
    fn new_blocks(
        &self,
        imported: Vec<H256>,
        _invalid: Vec<H256>,
        enacted: Vec<H256>,
        _retracted: Vec<H256>,
        sealed: Vec<H256>,
        _proposed: Vec<Vec<u8>>,
        _duration: u64,
    )
    {
        // if get_all_nodes_count() == 0 {
        //     return;
        // }

        if !imported.is_empty() {
            let min_imported_block_number = SyncStorage::get_synced_block_number() + 1;
            let mut max_imported_block_number = 0;
            let client = SyncStorage::get_block_chain();
            for hash in imported.iter() {
                let block_id = BlockId::Hash(*hash);
                if client.block_status(block_id) == BlockStatus::InChain {
                    if let Some(block_number) = client.block_number(block_id) {
                        if max_imported_block_number < block_number {
                            max_imported_block_number = block_number;
                        }
                    }
                }
            }

            // The imported blocks are not new or not yet in chain. Do not notify in this case.
            if max_imported_block_number < min_imported_block_number {
                return;
            }

            let synced_block_number = SyncStorage::get_synced_block_number();
            if max_imported_block_number <= synced_block_number {
                let mut hashes = Vec::new();
                for block_number in max_imported_block_number..synced_block_number + 1 {
                    let block_id = BlockId::Number(block_number);
                    if let Some(block_hash) = client.block_hash(block_id) {
                        hashes.push(block_hash);
                    }
                }
                if hashes.len() > 0 {
                    SyncStorage::remove_imported_block_hashes(hashes);
                }
            }

            SyncStorage::set_synced_block_number(max_imported_block_number);

            for block_number in min_imported_block_number..max_imported_block_number + 1 {
                let block_id = BlockId::Number(block_number);
                if let Some(blk) = client.block(block_id) {
                    let block_hash = blk.hash();
                    // import::import_staged_blocks(&block_hash);
                    if let Some(time) = SyncStorage::get_requested_time(&block_hash) {
                        info!(target: "sync",
                            "New block #{} {}, with {} txs added in chain, time elapsed: {:?}.",
                            block_number, block_hash, blk.transactions_count(), SystemTime::now().duration_since(time).expect("importing duration"));
                    }
                }
            }
        }

        if enacted.is_empty() {
            for hash in enacted.iter() {
                debug!(target: "sync", "enacted hash: {:?}", hash);
                // import::import_staged_blocks(&hash);
            }
        }

        if !sealed.is_empty() {
            debug!(target: "sync", "Propagating blocks...");
            SyncStorage::insert_imported_block_hashes(sealed.clone());
            // broadcast::propagate_blocks(sealed.index(0), SyncStorage::get_block_chain());
        }
    }

    fn start(&self) {
        info!(target: "sync", "starting...");
    }

    fn stop(&self) {
        info!(target: "sync", "stopping...");
    }

    fn broadcast(&self, _message: Vec<u8>) {}

    fn transactions_received(&self, transactions: &[Vec<u8>]) {
        if transactions.len() == 1 {
            let transaction_rlp = transactions[0].clone();
            if let Ok(tx) = UntrustedRlp::new(&transaction_rlp).as_val() {
                let transaction: UnverifiedTransaction = tx;
                let hash = transaction.hash();
                let sent_transaction_hashes_mutex = SyncStorage::get_sent_transaction_hashes();
                let mut lock = sent_transaction_hashes_mutex.lock();

                if let Ok(ref mut sent_transaction_hashes) = lock {
                    if !sent_transaction_hashes.contains_key(hash) {
                        sent_transaction_hashes.insert(hash.clone(), 0);
                        SyncStorage::insert_received_transaction(transaction_rlp);
                    }
                }
            }
        }
    }
}