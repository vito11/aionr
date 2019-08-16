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

use std::collections::{BTreeMap,HashMap,HashSet};
use std::ops::Index;
use std::sync::RwLock;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use std::time::SystemTime;
use rustc_hex::ToHex;
use client::BlockChainClient;
use client::BlockId;
use client::BlockStatus;
use client::ChainNotify;
use transaction::UnverifiedTransaction;
use aion_types::{H256,U256};
use futures::Future;
use futures::Stream;
use rlp::UntrustedRlp;
use tokio::runtime::Runtime;
use tokio::runtime::TaskExecutor;
use tokio::timer::Interval;
use bytes::BufMut;
use byteorder::{BigEndian,ByteOrder};

// chris
// use p2p::handler::external::Handler;
use p2p::Node;
use p2p::ChannelBuffer;
use p2p::Config;
// use p2p::register;
// use p2p::enable as p2p_start;
// use p2p::reset as p2p_shutdown;
// use p2p::get_nodes;
// use p2p::get_nodes_count;
// use p2p::get_all_nodes;
// use p2p::get_all_nodes_count;
// use p2p::get_local_node;
// use p2p::states::STATE::HANDSHAKEDONE;
// use p2p::states::STATE::CONNECTED;
// use p2p::states::STATE::ALIVE;
use p2p::Mgr;
use p2p::send;
use p2p::register;
use sync::route::VERSION;
use sync::route::MODULE;
use sync::route::ACTION;
use sync::handler::status;
use sync::handler::bodies;
use sync::handler::headers;
// use sync::handler::broadcast;
// use sync::handler::import;
use sync::helper::{Wrapper,WithStatus};
use sync::handler::headers::REQUEST_SIZE;
use header::Header;

use sync::storage::ActivePeerInfo;
use sync::storage::PeerInfo;
use sync::storage::SyncState;
use sync::storage::SyncStatus;
use sync::storage::SyncStorage;
use sync::storage::TransactionStats;
use p2p::{get_random_active_node_hash,get_random_active_node};

const HEADERS_CAPACITY: u64 = 256;
const STATUS_REQ_INTERVAL: u64 = 2;
const BLOCKS_BODIES_REQ_INTERVAL: u64 = 50;
const BLOCKS_IMPORT_INTERVAL: u64 = 50;
const BROADCAST_TRANSACTIONS_INTERVAL: u64 = 50;
const INTERVAL_STATUS: u64 = 10;
const INTERVAL_HEADERS: u64 = 2;
const INTERVAL_BODIES: u64 = 2;

#[derive(Clone)]
struct SyncMgr {}

impl SyncMgr {
    fn enable(executor: &TaskExecutor, max_peers: u32) {

        // let status_req_task =
        //     Interval::new(Instant::now(), Duration::from_secs(STATUS_REQ_INTERVAL))
        //         .for_each(move |_| {
        //             let active_nodes = get_nodes(ALIVE.value());
        //             for node in active_nodes.iter() {
        //                 trace!(target: "sync", "Sync status req sent...");
        //                 status::send(node.node_hash);
        //             }
        //             Ok(())
        //         })
        //         .map_err(|e| error!("interval errored; err={:?}", e));
        // executor.spawn(status_req_task);

        // let blocks_bodies_req_task = Interval::new(
        //     Instant::now(),
        //     Duration::from_millis(BLOCKS_BODIES_REQ_INTERVAL),
        // )
        // .for_each(move |_| {
        //     bodies::send();
        //     Ok(())
        // })
        // .map_err(|e| error!("interval errored; err={:?}", e));
        // executor.spawn(blocks_bodies_req_task);

        // let blocks_import_task = Interval::new(
        //     Instant::now(),
        //     Duration::from_millis(BLOCKS_IMPORT_INTERVAL),
        // )
        // .for_each(move |_| {
        //     import::import_blocks();
        //     Ok(())
        // })
        // .map_err(|e| error!("interval errored; err={:?}", e));
        // executor.spawn(blocks_import_task);

        // let broadcast_transactions_task = Interval::new(
        //     Instant::now(),
        //     Duration::from_millis(BROADCAST_TRANSACTIONS_INTERVAL),
        // )
        // .for_each(move |_| {
        //     broadcast::propagate_transactions();
        //     Ok(())
        // })
        // .map_err(|e| error!("interval errored; err={:?}", e));
        // executor.spawn(broadcast_transactions_task);

        // let statics_task = Interval::new(Instant::now(), Duration::from_secs(STATICS_INTERVAL))
        //     .for_each(move |_| {
        //         // let connected_nodes = get_nodes(CONNECTED.value());
        //         // for node in connected_nodes.iter() {
        //         //     if node.mode == Mode::BACKWARD || node.mode == Mode::FORWARD {
        //         //         if node.target_total_difficulty < SyncStorage::get_network_total_diff() {
        //         //             remove_peer(node.node_hash);
        //         //         }
        //         //     } else if node.last_request_timestamp
        //         //         + Duration::from_secs(STATICS_INTERVAL * 12)
        //         //         < SystemTime::now()
        //         //     {
        //         //         info!(target: "sync", "Disconnect with idle node: {}@{}.", node.get_node_id(), node.get_ip_addr());
        //         //         remove_peer(node.node_hash);
        //         //     }
        //         // }

        //         let chain_info = SyncStorage::get_chain_info();
        //         let block_number_last_time = SyncStorage::get_synced_block_number_last_time();
        //         let block_number_now = chain_info.best_block_number;
        //         let sync_speed = (block_number_now - block_number_last_time) / STATICS_INTERVAL;
        //         let mut active_nodes = get_nodes(ALIVE.value());
        //         let active_nodes_count = active_nodes.len();

        //         info!(target: "sync", "");
        //         info!(target: "sync", "{:=^127}", "  sync  ");
        //         info!(target: "sync", "  local num:{:>11}, hash:{:>20}", chain_info.best_block_number, chain_info.best_block_hash);
        //         info!(target: "sync", "network num:{:>11}, hash:{:>20}", SyncStorage::get_network_best_block_number(), SyncStorage::get_network_best_block_hash());
        //         info!(target: "sync", " staged num:{:>11}", SyncStorage::get_max_staged_block_number());
        //         info!(target: "sync", "       sync:{:>11} blks/sec", sync_speed);
        //         info!(target: "sync",
        //             "total/connected/active peers: {}/{}/{}",
        //             get_all_nodes_count(),
        //             get_nodes_count(CONNECTED.value()),
        //             active_nodes_count,
        //         );

        //         if active_nodes_count > 0 {
        //             info!(target: "sync", "{:-^127}","");
        //             info!(target: "sync","              td         bn          bh                         addr                 rev      conn  seed      lst-req          m");
        //             info!(target: "sync", "{:-^127}","");
        //             active_nodes.sort_by(|a, b| {
        //                 if a.target_total_difficulty != b.target_total_difficulty {
        //                     b.target_total_difficulty.cmp(&a.target_total_difficulty)
        //                 } else {
        //                     b.best_block_num.cmp(&a.best_block_num)
        //                 }
        //             });
        //             let mut count: u32 = 0;
        //             for node in active_nodes.iter() {
        //                 if let Ok(_) = node.last_request_timestamp.elapsed() {
        //                     info!(target: "sync",
        //                         "{:>16}{:>11}{:>12}{:>24}{:>25}{:>10}{:>6}{:>12}{:>11}",
        //                         format!("{}",node.target_total_difficulty),
        //                         node.best_block_num,
        //                         format!("{}",node.best_hash),
        //                         node.get_display_ip_addr(),
        //                         String::from_utf8_lossy(&node.revision).trim(),
        //                         match node.ip_addr.is_server{
        //                             true => "Outbound",
        //                             _=>"Inbound"
        //                         },
        //                         match node.is_from_boot_list{
        //                             true => "Y",
        //                             _ => ""
        //                         },
        //                         node.last_request_num,
        //                         format!("{}",node.mode)
        //                     );
        //                     count += 1;
        //                     if count ==  max_peers {
        //                         break;
        //                     }
        //                 }
        //             }
        //             info!(target: "sync", "{:-^127}","");
        //         }

        //         if block_number_now + 8 < SyncStorage::get_network_best_block_number()
        //             && block_number_now - block_number_last_time < 2
        //         {
        //             SyncStorage::get_block_chain().clear_queue();
        //             SyncStorage::get_block_chain().clear_bad();
        //             SyncStorage::clear_downloaded_headers();
        //             SyncStorage::clear_downloaded_blocks();
        //             SyncStorage::clear_downloaded_block_hashes();
        //             SyncStorage::clear_requested_blocks();
        //             SyncStorage::clear_headers_with_bodies_requested();
        //             SyncStorage::set_synced_block_number(
        //                 SyncStorage::get_chain_info().best_block_number,
        //             );
        //             // let abnormal_mode_nodes_count =
        //             //     get_nodes_count_with_mode(Mode::BACKWARD)
        //             //         + get_nodes_count_with_mode(Mode::FORWARD);
        //             // if abnormal_mode_nodes_count > (active_nodes_count / 5)
        //             //     || active_nodes_count == 0
        //             // {
        //             //     info!(target: "sync", "Abnormal status, reseting network...");
        //             //     reset();

        //             //     SyncStorage::clear_imported_block_hashes();
        //             //     SyncStorage::clear_staged_blocks();
        //             //     SyncStorage::set_max_staged_block_number(0);
        //             // }
        //         }

        //         // if block_number_now + 8 < SyncStorage::get_network_best_block_number()
        //         //     && block_number_now - block_number_last_time < 2
        //         // {
        //         //     SyncStorage::get_block_chain().clear_queue();
        //         //     SyncStorage::get_block_chain().clear_bad();
        //         //     SyncStorage::clear_downloaded_headers();
        //         //     SyncStorage::clear_downloaded_blocks();
        //         //     SyncStorage::clear_downloaded_block_hashes();
        //         //     SyncStorage::clear_requested_blocks();
        //         //     SyncStorage::clear_headers_with_bodies_requested();
        //         //     SyncStorage::set_synced_block_number(
        //         //         SyncStorage::get_chain_info().best_block_number,
        //         //     );
        //         //     let abnormal_mode_nodes_count =
        //         //         get_nodes_count_with_mode(Mode::BACKWARD)
        //         //             + get_nodes_count_with_mode(Mode::FORWARD);
        //         //     if abnormal_mode_nodes_count > (active_nodes_count / 5)
        //         //         || active_nodes_count == 0
        //         //     {
        //         //         info!(target: "sync", "Abnormal status, reseting network...");
        //         //         reset();

        //         //         SyncStorage::clear_imported_block_hashes();
        //         //         SyncStorage::clear_staged_blocks();
        //         //         SyncStorage::set_max_staged_block_number(0);
        //         //     }
        //         // }
        //         // ------
        //         // FIX: abnormal reset will be triggered in chain reorg.
        //         //   block_number_now is the local best block
        //         //   network_best_block_number is the network best block
        //         //   block_number_last_time is the local best block set last time where these codes are executed
        //         //   when doing a deep chain reorg, if the BACKWARD and FORWARD syncing can't finish within one data batch, block_number_now acctually won't change
        //         //   so we will have block_number_now == block_number_last_time, and block_number_now smaller than network_best_block_number.
        //         //   This condition triggers reset but it's not abnormal.
        //         // PoC disabled it. We should fix it.
        //         // ------

        //         SyncStorage::set_synced_block_number_last_time(block_number_now);
        //         SyncStorage::set_sync_speed(sync_speed as u16);

        //         if SyncStorage::get_network_best_block_number()
        //             <= SyncStorage::get_synced_block_number()
        //         {
        //             // full synced
        //             SyncStorage::clear_staged_blocks();
        //         }

        //         Ok(())
        //     })
        //     .map_err(|e| error!("interval errored; err={:?}", e));
        // executor.spawn(statics_task);
    }

    fn handle(node: &mut Node, req: ChannelBuffer) {
        // if node.state_code & HANDSHAKEDONE.value() != HANDSHAKEDONE.value() {
        //     return;
        // }

        // match VERSION::from(req.head.ver) {
        //     VERSION::V0 => {
        //         trace!(target: "sync", "version {0} module {1} action{2}",
        //             req.head.ver,
        //             req.head.ctrl,
        //             req.head.action
        //         );
        //         match ACTION::from(req.head.action) {
        //             ACTION::STATUSREQ => {
        //                 status::receive_req(node);
        //             }
        //             ACTION::STATUSRES => {
        //                 status::receive_res(node, req);
        //             }
        //             ACTION::HEADERSREQ => {
        //                 headers::handle_blocks_headers_req(node, req);
        //             }
        //             ACTION::HEADERSRES => {
        //                 headers::handle_blocks_headers_res(node, req);
        //             }
        //             ACTION::BODIESREQ => {
        //                 bodies::receive_req(node, req);
        //             }
        //             ACTION::BODIESRES => {
        //                 bodies::receive_res(node, req);
        //             }
        //             ACTION::BROADCASTTX => {
        //                 broadcast::receive_tx(node, req);
        //             }
        //             ACTION::BROADCASTBLOCK => {
        //                 broadcast::receive_block(node, req);
        //             }
        //             _ => {
        //                 trace!(target: "sync", "UNKNOWN received.");
        //             }
        //         }
        //     }
        //     VERSION::V1 => {
        //         trace!(target: "sync", "Ver 1 package received.");
        //     }
        // };
    }

    fn disable() {
        //SyncStorage::reset();
    }
}

pub struct Sync {
    config: Arc<Config>,
    client: Arc<BlockChainClient>,
    runtime: Arc<Runtime>,
    p2p: Arc<Mgr>,

    // TODO: avoid the same type req to one node
    //    working_nodes : Arc<RwLock<HashSet<u64>>>,
    wrappers: Arc<RwLock<BTreeMap<u64, Wrapper>>>,

    /// network best td
    td: Arc<RwLock<U256>>,
}

impl Sync {
    pub fn new(config: Config, client: Arc<BlockChainClient>) -> Sync {
        let starting_td = client.chain_info().total_difficulty;
        // TODO: remove
        // SyncStorage::init(client);
        let config = Arc::new(config);
        Sync {
            config: config.clone(),
            client,
            p2p: Arc::new(Mgr::new(config)),
            runtime: Arc::new(Runtime::new().expect("tokio runtime")),
            wrappers: Arc::new(RwLock::new(BTreeMap::new())),
            td: Arc::new(RwLock::new(starting_td)),
        }
    }

    pub fn run(&self) {
        // counters
        let runtime = self.runtime.clone();
        let executor = Arc::new(runtime.executor());
        let nodes = self.p2p.nodes.clone();
        //        let mut handlers = self.p2p.handlers.clone();

        // register handlers
        //        register(0, 1, 0, &mut handlers, status::receive_req);
        //        register(0, 1, 1, &mut handlers, status::receive_res);
        //         register(0, 1, 2, &mut handlers, headers::receive_req);
        //         register(0, 1, 3, &mut handlers, headers::receive_res);
        // register(0, 1, 4, &mut handlers, status::receive_req);
        // register(0, 1, 5, &mut handlers, status::receive_res);
        // register(0, 1, 6, &mut handlers, status::receive_req);
        // register(0, 1, 7, &mut handlers, status::receive_res);

        // init p2p
        &self.p2p.run(Arc::new(handle), self.wrappers.clone());

        // status
        let executor_status = executor.clone();
        let nodes_status = nodes.clone();
        let nodes_headers = nodes.clone();
        let nodes_send1 = nodes.clone();
        let nodes_bodies = nodes.clone();
        executor_status.spawn(
            Interval::new(Instant::now(), Duration::from_secs(INTERVAL_STATUS))
                .for_each(move |_| {
                    // make it constant
                    status::send(nodes_status.clone());
                    //                     p2p.get_node_by_td(10);
                    Ok(())
                })
                .map_err(|err| error!(target: "p2p", "executor status: {:?}", err)),
        );
        let executor_headers = executor.clone();
        let wrappers1 = self.wrappers.clone();
        let client = self.client.clone();
        executor_headers.spawn(
            Interval::new(Instant::now(), Duration::from_secs(INTERVAL_HEADERS))
                .for_each(move |_| {
                    // make it constant
                    let chain_info = client.chain_info();
                    let mut max = 0u64;
                    if let Ok(read) = wrappers1.read() {
                        max = read.keys().last().map_or(0u64, |x| x.clone());
                    };
                    if max < chain_info.best_block_number + HEADERS_CAPACITY {
                        if let Some(node) = get_random_active_node(nodes_headers.clone()) {
                            if node.total_difficulty > chain_info.total_difficulty
                                && node.block_num - REQUEST_SIZE as u64
                                    >= chain_info.best_block_number
                            {
                                let start = if max != 0 {
                                    max
                                } else if chain_info.best_block_number > 3 {
                                    chain_info.best_block_number - 3
                                } else {
                                    1
                                };

                                headers::send(start, node.get_hash(), nodes_send1.clone());
                            }
                        }
                    }
                    //                     p2p.get_node_by_td(10);
                    Ok(())
                })
                .map_err(|err| error!(target: "p2p", "executor headers: {:?}", err)),
        );
        let executor_bodies = executor.clone();
        let wrappers2 = self.wrappers.clone();
        executor_bodies.spawn(
            Interval::new(Instant::now(), Duration::from_secs(INTERVAL_BODIES))
                .for_each(move |_| {
                    if let Ok(mut wrappers) = wrappers2.try_write() {
                        if let Some((num, wrapper)) = wrappers
                            .clone()
                            .iter()
                            .filter(|(_, w)| {
                                match w.with_status {
                                    WithStatus::GetHeader(_) => true,
                                    _ => false,
                                }
                            })
                            .next()
                        {
                            match wrapper.with_status {
                                WithStatus::GetHeader(ref hw) => {
                                    let mut cb = ChannelBuffer::new();
                                    cb.head.ver = VERSION::V0.value();
                                    cb.head.ctrl = MODULE::SYNC.value();
                                    cb.head.action = ACTION::BODIESREQ.value();
                                    for h in hw.clone() {
                                        let rlp = UntrustedRlp::new(&h);
                                        let header: Header =
                                            rlp.as_val().expect("should not be err");
                                        cb.body.put_slice(&header.hash());
                                    }
                                    cb.head.len = cb.body.len() as u32;
                                    send(num, cb, nodes_bodies.clone());
                                    if let Some(w) = wrappers.get_mut(num) {
                                        (*w).timestamp = SystemTime::now();
                                        (*w).with_status = WithStatus::WaitForBody(hw.clone());
                                    };
                                }
                                _ => (),
                            };
                        }
                    }
                    Ok(())
                })
                .map_err(|err| error!(target: "p2p", "executor bodies: {:?}", err)),
        );
        //        let executor_import = executor.clone();
        //        executor_bodies.spawn(
        //            Interval::new(Instant::now(), Duration::from_secs(INTERVAL_BODIES))
        //                .for_each(move |_| {
        //                    if let Ok(mut wrappers) = wrappers2.try_write(){
        //                        if let Some((num,wrapper)) = wrappers.clone()
        //                            .iter()
        //                            .filter(|(_,w)| match w.with_status { WithStatus::GetHeader(_) => true, _ => false })
        //                            .next()
        //                            {
        //                                match wrapper.with_status {
        //                                    WithStatus::GetHeader(ref hw) => {
        //                                        let mut cb = ChannelBuffer::new();
        //                                        cb.head.ver = VERSION::V0.value();
        //                                        cb.head.ctrl = MODULE::SYNC.value();
        //                                        cb.head.action = ACTION::BODIESREQ.value();
        //                                        for h in hw.clone() {
        //                                            let rlp = UntrustedRlp::new(&h);
        //                                            let header:Header = rlp.as_val().expect("should not be err");
        //                                            cb.body.put_slice(&header.hash());
        //                                        }
        //                                        cb.head.len = cb.body.len() as u32;
        //                                        send(num,cb,nodes_bodies.clone());
        //                                        if let Some(w) =wrappers.get_mut(num){
        //                                            (*w).timestamp = SystemTime::now();
        //                                            (*w).with_status = WithStatus::WaitForBody(hw.clone());
        //                                        };
        //                                    }
        //                                    _ => ()
        //                                };
        //                            }
        //                    }
        //                    Ok(())
        //                })
        //                .map_err(|err| error!(target: "p2p", "executor status: {:?}", err)),
        //        );
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
    ws: Arc<RwLock<BTreeMap<u64, Wrapper>>>,
)
{
    match ACTION::from(cb.head.action) {
        ACTION::STATUSREQ => {
            if cb.head.len != 0 {
                // TODO: kill the node
            }
            status::receive_req(hash, nodes)
        }
        ACTION::STATUSRES => status::receive_res(hash, cb, nodes),
        ACTION::HEADERSREQ => headers::receive_req(hash, cb, nodes),
        ACTION::HEADERSRES => headers::receive_res(hash, cb, nodes, ws),
        ACTION::BODIESREQ => bodies::receive_req(hash, cb, nodes),
        ACTION::BODIESRES => bodies::receive_res(hash, cb, nodes, ws),
        ACTION::BROADCASTTX => (),
        ACTION::BROADCASTBLOCK => (),
        // TODO: kill the node
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
