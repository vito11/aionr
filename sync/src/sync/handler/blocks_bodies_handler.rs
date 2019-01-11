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

use acore::block::Block;
use acore::client::BlockId;
use aion_types::H256;
use bytes::BufMut;
use rlp::{RlpStream, UntrustedRlp};
use std::time::SystemTime;

use super::super::action::SyncAction;
use super::super::event::SyncEvent;
use super::super::storage::{BlocksWrapper, SyncStorage};
use p2p::*;

use super::blocks_headers_handler::BlockHeadersHandler;

const HASH_LEN: usize = 32;

pub struct BlockBodiesHandler;

impl BlockBodiesHandler {
    pub fn send_blocks_bodies_req() {
        let mut req = ChannelBuffer::new();
        req.head.ver = Version::V0.value();
        req.head.ctrl = Control::SYNC.value();
        req.head.action = SyncAction::BLOCKSBODIESREQ.value();

        let mut hws = Vec::new();
        if let Ok(mut downloaded_headers) = SyncStorage::get_downloaded_headers().try_lock() {
            while let Some(hw) = downloaded_headers.pop_front() {
                if !hw.headers.is_empty() {
                    hws.push(hw);
                }
            }
        }

        for hw in hws.iter() {
            let mut req = req.clone();
            req.body.clear();

            for header in hw.headers.iter() {
                if !SyncStorage::is_imported_block_hash(&header.hash()) {
                    req.body.put_slice(&header.hash());
                }
            }

            let body_len = req.body.len();
            if body_len > 0 {
                if let Ok(ref mut headers_with_bodies_requested) =
                    SyncStorage::get_headers_with_bodies_requested().lock()
                {
                    if !headers_with_bodies_requested.contains_key(&hw.node_hash) {
                        req.head.set_length(body_len as u32);

                        P2pMgr::send(hw.node_hash, req);

                        trace!(target: "sync", "Sync blocks bodies req sent...");
                        let mut hw = hw.clone();
                        hw.timestamp = SystemTime::now();
                        headers_with_bodies_requested.insert(hw.node_hash, hw);
                    }
                }
            }
        }
    }

    pub fn handle_blocks_bodies_req(node: &mut Node, req: ChannelBuffer) {
        trace!(target: "sync", "BLOCKSBODIESREQ received.");

        let mut res = ChannelBuffer::new();
        let node_hash = node.node_hash;

        res.head.ver = Version::V0.value();
        res.head.ctrl = Control::SYNC.value();
        res.head.action = SyncAction::BLOCKSBODIESRES.value();

        let mut res_body = Vec::new();
        let hash_count = req.body.len() / HASH_LEN;
        let mut rest = req.body.as_slice();
        let mut data = Vec::new();
        let mut body_count = 0;
        let client = SyncStorage::get_block_chain();
        for _i in 0..hash_count {
            let (hash, next) = rest.split_at(HASH_LEN);

            match client.block_body(BlockId::Hash(H256::from(hash))) {
                Some(bb) => {
                    data.append(&mut bb.into_inner());
                    body_count += 1;
                }
                None => {}
            }

            rest = next;
        }

        if body_count > 0 {
            let mut rlp = RlpStream::new_list(body_count);
            rlp.append_raw(&data, body_count);
            res_body.put_slice(rlp.as_raw());
        }

        res.body.put_slice(res_body.as_slice());
        res.head.set_length(res.body.len() as u32);

        SyncEvent::update_node_state(node, SyncEvent::OnBlockBodiesReq);
        P2pMgr::update_node(node_hash, node);
        P2pMgr::send(node_hash, res);
    }

    pub fn handle_blocks_bodies_res(node: &mut Node, req: ChannelBuffer) {
        trace!(target: "sync", "BLOCKSBODIESRES received from: {}.", node.get_ip_addr());

        let node_hash = node.node_hash;
        let mut blocks = Vec::new();
        if req.body.len() > 0 {
            match SyncStorage::pick_headers_with_bodies_requested(&node_hash) {
                Some(hw) => {
                    let headers = hw.headers;
                    if !headers.is_empty() {
                        let rlp = UntrustedRlp::new(req.body.as_slice());

                        let mut bodies = Vec::new();
                        for block_bodies in rlp.iter() {
                            for block_body in block_bodies.iter() {
                                let mut transactions = Vec::new();
                                if !block_body.is_empty() {
                                    for transaction_rlp in block_body.iter() {
                                        if !transaction_rlp.is_empty() {
                                            if let Ok(transaction) = transaction_rlp.as_val() {
                                                transactions.push(transaction);
                                            }
                                        }
                                    }
                                }
                                bodies.push(transactions);
                            }
                        }

                        if headers.len() == bodies.len() {
                            for i in 0..headers.len() {
                                let block = Block {
                                    header: headers[i].clone(),
                                    transactions: bodies[i].clone(),
                                };
                                blocks.push(block);
                            }
                        } else {
                            debug!(
                                target: "sync",
                                "Count mismatch, headers count: {}, bodies count: {}, node id: {}",
                                headers.len(),
                                bodies.len(),
                                node.get_node_id()
                            );
                            blocks.clear();
                        }

                        if !blocks.is_empty() {
                            let mut bw = BlocksWrapper::new();
                            bw.node_id_hash = node.node_hash;
                            bw.blocks.extend(blocks);
                            SyncStorage::insert_downloaded_blocks(bw);
                        }
                    }
                }
                None => {}
            }
        }

        BlockHeadersHandler::get_headers_from_node(node);

        SyncEvent::update_node_state(node, SyncEvent::OnBlockBodiesRes);
        P2pMgr::update_node(node_hash, node);
    }
}
