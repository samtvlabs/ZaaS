// Copyright 2023 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use core::mem;

use anyhow::{bail, Result};
use hashbrown::HashMap;
use revm::primitives::{AccountInfo, Bytecode, B256};
use zeth_primitives::{
    keccak::{keccak, KECCAK_EMPTY},
    revm::to_revm_b256,
    trie::StateAccount,
    Bytes,
};

use crate::{
    block_builder::BlockBuilder,
    consts::MAX_BLOCK_HASH_AGE,
    guest_mem_forget,
    mem_db::{AccountState, DbAccount, MemDb},
    NoHashBuilder,
};

pub trait DbInitStrategy {
    type Db;

    fn initialize_database(block_builder: BlockBuilder<Self::Db>)
        -> Result<BlockBuilder<Self::Db>>;
}

pub struct MemDbInitStrategy {}

impl DbInitStrategy for MemDbInitStrategy {
    type Db = MemDb;

    fn initialize_database(
        mut block_builder: BlockBuilder<Self::Db>,
    ) -> Result<BlockBuilder<Self::Db>> {
        // Verify state trie root
        if block_builder.input.parent_state_trie.hash()
            != block_builder.input.parent_header.state_root
        {
            bail!(
                "Invalid state trie: expected {}, got {}",
                block_builder.input.parent_header.state_root,
                block_builder.input.parent_state_trie.hash()
            );
        }

        // hash all the contract code
        let contracts: HashMap<B256, Bytes, NoHashBuilder> =
            mem::take(&mut block_builder.input.contracts)
                .into_iter()
                .map(|bytes| (keccak(&bytes).into(), bytes))
                .collect();

        // Load account data into db
        let mut accounts = HashMap::with_capacity_and_hasher(
            block_builder.input.parent_storage.len(),
            NoHashBuilder::default(),
        );
        for (address, (storage_trie, slots)) in &mut block_builder.input.parent_storage {
            // consume the slots, as they are no longer needed afterwards
            let slots = mem::take(slots);

            // load the account from the state trie or empty if it does not exist
            let state_account = block_builder
                .input
                .parent_state_trie
                .get_rlp::<StateAccount>(&keccak(address))?
                .unwrap_or_default();
            // Verify storage trie root
            if storage_trie.hash() != state_account.storage_root {
                bail!(
                    "Invalid storage trie for {:?}: expected {}, got {}",
                    address,
                    state_account.storage_root,
                    storage_trie.hash()
                );
            }

            // load the corresponding code
            let code_hash = to_revm_b256(state_account.code_hash);
            let bytecode = if code_hash.0 == KECCAK_EMPTY.0 {
                Bytecode::new()
            } else {
                let bytes = contracts.get(&code_hash).unwrap().clone();
                unsafe { Bytecode::new_raw_with_hash(bytes.0, code_hash) }
            };

            // load storage reads
            let mut storage = HashMap::with_capacity(slots.len());
            for slot in slots {
                let value: zeth_primitives::U256 = storage_trie
                    .get_rlp(&keccak(slot.to_be_bytes::<32>()))?
                    .unwrap_or_default();
                storage.insert(slot, value);
            }

            let mem_account = DbAccount {
                info: AccountInfo {
                    balance: state_account.balance,
                    nonce: state_account.nonce,
                    code_hash: to_revm_b256(state_account.code_hash),
                    code: Some(bytecode),
                },
                state: AccountState::None,
                storage,
            };

            accounts.insert(*address, mem_account);
        }
        guest_mem_forget(contracts);

        // prepare block hash history
        let mut block_hashes =
            HashMap::with_capacity(block_builder.input.ancestor_headers.len() + 1);
        block_hashes.insert(
            block_builder.input.parent_header.number,
            to_revm_b256(block_builder.input.parent_header.hash()),
        );
        let mut prev = &block_builder.input.parent_header;
        for current in &block_builder.input.ancestor_headers {
            let current_hash = current.hash();
            if prev.parent_hash != current_hash {
                bail!(
                    "Invalid chain: {} is not the parent of {}",
                    current.number,
                    prev.number
                );
            }
            if block_builder.input.parent_header.number < current.number
                || block_builder.input.parent_header.number - current.number >= MAX_BLOCK_HASH_AGE
            {
                bail!(
                    "Invalid chain: {} is not one of the {} most recent blocks",
                    current.number,
                    MAX_BLOCK_HASH_AGE,
                );
            }
            block_hashes.insert(current.number, to_revm_b256(current_hash));
            prev = current;
        }

        // Store database
        Ok(block_builder.with_db(MemDb {
            accounts,
            block_hashes,
        }))
    }
}
