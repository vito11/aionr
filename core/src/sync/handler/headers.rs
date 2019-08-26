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

use std::mem;
use std::time::{SystemTime, Duration};
use std::sync::Arc;
use std::collections::{HashMap, VecDeque};

use parking_lot::{Mutex, RwLock};

use engine::unity_engine::UnityEngine;
use header::Header;
use acore_bytes::to_hex;
use aion_types::U256;
use client::{BlockChainClient, BlockId};
use byteorder::{BigEndian, ByteOrder, ReadBytesExt};
use bytes::BufMut;
use rlp::{RlpStream, UntrustedRlp};
use p2p::{ChannelBuffer, Mgr, Node};
use sync::route::{VERSION, MODULE, ACTION};
use sync::wrappers::{HeadersWrapper};
use sync::node_info::{NodeInfo, Mode};
use sync::storage::SyncStorage;
use rand::{thread_rng, Rng};

const NORMAL_REQUEST_SIZE: u32 = 24;
const LARGE_REQUEST_SIZE: u32 = 40;
const REQUEST_COOLDOWN: u64 = 5000;
const BACKWARD_SYNC_STEP: u64 = NORMAL_REQUEST_SIZE as u64 * 6 - 1;
const FAR_OVERLAPPING_BLOCKS: u64 = 3;
const CLOSE_OVERLAPPING_BLOCKS: u64 = 15;

pub fn sync_headers(
    p2p: Mgr,
    nodes_info: Arc<RwLock<HashMap<u64, RwLock<NodeInfo>>>>,
    local_total_diff: &U256,
    local_best_block_number: u64,
)
{
    let active_nodes = p2p.get_active_nodes();
    // Filter nodes. Only sync from nodes with higher total difficulty and with a cooldown restriction.
    let candidates: Vec<Node> =
        filter_nodes_to_sync_headers(active_nodes, nodes_info.clone(), local_total_diff);
    // Pick a random node among all candidates
    if let Some(candidate) = pick_random_node(&candidates) {
        let candidate_hash = candidate.get_hash();
        let nodes_info_read = nodes_info.read();
        if let Some(node_info_lock) = nodes_info_read.get(&candidate_hash) {
            let mut node_info = node_info_lock.write();
            // Send header request
            if prepare_send(p2p, candidate_hash, &node_info, local_best_block_number) {
                // Update cooldown time after request succesfully sent
                node_info.last_headers_request_time = SystemTime::now();
            }
        }
    }
}

fn prepare_send(p2p: Mgr, node_hash: u64, node_info: &NodeInfo, local_best_numbder: u64) -> bool {
    let node_best_number = node_info.best_block_number;
    let from: u64;
    let mut size: u32 = NORMAL_REQUEST_SIZE;

    match node_info.mode {
        Mode::THUNDER => {
            // TODO: add repeat threshold
            from = if local_best_numbder > FAR_OVERLAPPING_BLOCKS {
                local_best_numbder - FAR_OVERLAPPING_BLOCKS
            } else {
                1
            };
            size = LARGE_REQUEST_SIZE;
        }
        Mode::NORMAL => {
            if node_best_number >= local_best_numbder + BACKWARD_SYNC_STEP {
                from = if local_best_numbder > FAR_OVERLAPPING_BLOCKS {
                    local_best_numbder - FAR_OVERLAPPING_BLOCKS
                } else {
                    1
                };
            } else {
                from = if local_best_numbder > CLOSE_OVERLAPPING_BLOCKS {
                    local_best_numbder - CLOSE_OVERLAPPING_BLOCKS
                } else {
                    1
                };
            }
        }
    }

    send(p2p.clone(), node_hash, from, size)
}

fn send(p2p: Mgr, hash: u64, from: u64, size: u32) -> bool {
    debug!(target:"sync","headers.rs/send: from {}, size: {}, node hash: {}", from, size, hash);
    let mut cb = ChannelBuffer::new();
    cb.head.ver = VERSION::V0.value();
    cb.head.ctrl = MODULE::SYNC.value();
    cb.head.action = ACTION::HEADERSREQ.value();

    let mut from_buf = [0u8; 8];
    BigEndian::write_u64(&mut from_buf, from);
    cb.body.put_slice(&from_buf);

    let mut size_buf = [0u8; 4];
    BigEndian::write_u32(&mut size_buf, size);
    cb.body.put_slice(&size_buf);

    cb.head.len = cb.body.len() as u32;
    p2p.send(hash, cb)
}

// pub fn send(
//     p2p: Arc<Mgr>,
//     start: u64,
//     chain_info: &BlockChainInfo,
//     ws: Arc<RwLock<HashMap<u64, HeadersWrapper>>>,
// )
// {
//     let working_nodes = get_working_nodes(ws);

//     if let Some(node) = p2p.get_random_active_node(&working_nodes) {

//         if node.total_difficulty > chain_info.total_difficulty
//             && node.block_num - REQUEST_SIZE as u64 >= chain_info.best_block_number
//         {
//             let start = if start > 3 {
//                 start - 3
//             } else if chain_info.best_block_number > 3 {
//                 chain_info.best_block_number - 3
//             } else {
//                 1
//             };
//             debug!(target:"sync","send header req start: {} , size: {} , node_hash: {}", start, REQUEST_SIZE,node.hash);
//             let mut cb = ChannelBuffer::new();
//             cb.head.ver = VERSION::V0.value();
//             cb.head.ctrl = MODULE::SYNC.value();
//             cb.head.action = ACTION::HEADERSREQ.value();

//             let mut from_buf = [0u8; 8];
//             BigEndian::write_u64(&mut from_buf, start);
//             cb.body.put_slice(&from_buf);

//             let mut size_buf = [0u8; 4];
//             BigEndian::write_u32(&mut size_buf, REQUEST_SIZE);
//             cb.body.put_slice(&size_buf);

//             cb.head.len = cb.body.len() as u32;
//             p2p.send(p2p.clone(), node.hash, cb);
//         }
//     }
// }

pub fn receive_req(p2p: Mgr, hash: u64, client: Arc<BlockChainClient>, cb_in: ChannelBuffer) {
    trace!(target: "sync", "headers/receive_req");

    let mut res = ChannelBuffer::new();

    res.head.ver = VERSION::V0.value();
    res.head.ctrl = MODULE::SYNC.value();
    res.head.action = ACTION::HEADERSRES.value();

    let mut res_body = Vec::new();

    let (mut from, req_body_rest) = cb_in.body.split_at(mem::size_of::<u64>());
    let from = from.read_u64::<BigEndian>().unwrap_or(1);
    let (mut size, _) = req_body_rest.split_at(mem::size_of::<u32>());
    let size = size.read_u32::<BigEndian>().unwrap_or(1);
    let mut data = Vec::new();

    if size <= LARGE_REQUEST_SIZE {
        for i in from..(from + size as u64) {
            match client.block_header(BlockId::Number(i)) {
                Some(hdr) => {
                    data.append(&mut hdr.into_inner());
                }
                None => {
                    break;
                }
            }
        }

        if data.len() > 0 {
            let mut rlp = RlpStream::new_list(data.len() as usize);
            rlp.append_raw(&data, data.len() as usize);
            res_body.put_slice(rlp.as_raw());
        }

        res.body.put_slice(res_body.as_slice());
        res.head.len = res.body.len() as u32;

        p2p.update_node(&hash);
        p2p.send(hash, res);
    } else {
        warn!(target:"sync","headers/receive_req max headers size requested");
        return;
    }
}

pub fn receive_res(p2p: Mgr, hash: u64, cb_in: ChannelBuffer, storage: Arc<SyncStorage>) {
    trace!(target: "sync", "headers/receive_res");

    let downloaded_headers: &Mutex<VecDeque<HeadersWrapper>> = storage.downloaded_headers();

    let rlp = UntrustedRlp::new(cb_in.body.as_slice());
    let mut prev_header = Header::new();
    let mut header_wrapper = HeadersWrapper::new();
    let mut headers = Vec::new();

    for header_rlp in rlp.iter() {
        if let Ok(header) = header_rlp.as_val() {
            let result = UnityEngine::validate_block_header(&header);
            match result {
                Ok(()) => {
                    // break if not consisting
                    if prev_header.number() != 0
                        && (header.number() != prev_header.number() + 1
                            || prev_header.hash() != *header.parent_hash())
                    {
                        error!(target: "sync",
                            "<inconsistent-block-headers num={}, prev+1={}, hash={}, p_hash={}>, hash={}>",
                            header.number(),
                            prev_header.number() + 1,
                            header.parent_hash(),
                            prev_header.hash(),
                            header.hash(),
                        );
                        break;
                    } else {
                        let block_hash = header.hash();

                        // let number = header.number();

                        // Skip staged block header
                        // if node.mode == Mode::THUNDER {
                        //     if SyncStorage::is_staged_block_hash(hash) {
                        //         debug!(target: "sync", "Skip staged block header #{}: {:?}", number, hash);
                        //         // hw.headers.push(header.clone());
                        //         break;
                        //     }
                        // }

                        // ignore the block if its body is already downloaded or imported
                        if !storage.is_block_hash_downloaded(&block_hash)
                            && !storage.is_block_hash_imported(&block_hash)
                        {
                            headers.push(header.clone());
                        }
                    }
                    prev_header = header;
                }
                Err(e) => {
                    // ignore this batch if any invalidated header
                    error!(target: "sync", "Invalid header: {:?}, header: {}", e, to_hex(header_rlp.as_raw()));
                }
            }
        } else {
            error!(target: "sync", "Invalid header: {}", to_hex(header_rlp.as_raw()));
        }
    }

    if !headers.is_empty() {
        header_wrapper.node_hash = hash;
        header_wrapper.headers = headers;
        header_wrapper.timestamp = SystemTime::now();
        p2p.update_node(&hash);
        let mut downloaded_headers = downloaded_headers.lock();
        downloaded_headers.push_back(header_wrapper);
    } else {
        debug!(target: "sync", "Came too late............");
    }
}

/// Filter candidates to sync from based on total difficulty and syncing cool down
fn filter_nodes_to_sync_headers(
    nodes: Vec<Node>,
    nodes_info: Arc<RwLock<HashMap<u64, RwLock<NodeInfo>>>>,
    local_total_diff: &U256,
) -> Vec<Node>
{
    let time_now = SystemTime::now();
    let nodes_info_read = nodes_info.read();
    nodes
        .into_iter()
        .filter(|node| {
            let node_hash = node.get_hash();
            nodes_info_read
                .get(&node_hash)
                .map_or(false, |node_info_lock| {
                    let node_info = node_info_lock.read();
                    &node_info.total_difficulty > local_total_diff
                        && node_info.last_headers_request_time
                            + Duration::from_millis(REQUEST_COOLDOWN)
                            <= time_now
                })
        })
        .collect()
}

/// Pick a random node
fn pick_random_node(nodes: &Vec<Node>) -> Option<Node> {
    let count = nodes.len();
    if count > 0 {
        let mut rng = thread_rng();
        let random_index: usize = rng.gen_range(0, count);
        Some(nodes[random_index].clone())
    } else {
        None
    }
}
