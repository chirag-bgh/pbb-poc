use std::sync::Arc;

use pevm::{EvmCode, TransactTo};
use reth_primitives::revm_primitives::Bytecode;
use reth_primitives::{Bytes, JumpTable, Transaction, TransactionSigned, TxKind, U256};
use reth_revm::interpreter::opcode;
use reth_revm::primitives::bitvec::bitvec;
use reth_revm::primitives::bitvec::order::Lsb0;
use reth_revm::primitives::bitvec::vec::BitVec;
use reth_revm::primitives::LegacyAnalyzedBytecode;

pub fn get_tx_env(tx_signed: TransactionSigned) -> pevm::TxEnv {
    let mut tx_env = pevm::TxEnv::default();

    tx_env.caller = tx_signed.recover_signer().unwrap();

    match tx_signed.as_ref() {
        Transaction::Legacy(tx) => {
            tx_env.gas_limit = tx.gas_limit;
            tx_env.gas_price = U256::from(tx.gas_price);
            tx_env.gas_priority_fee = None;
            tx_env.transact_to = match tx.to {
                TxKind::Call(to) => TransactTo::Call(to),
                TxKind::Create => TransactTo::Create,
            };
            tx_env.value = tx.value;
            tx_env.data = tx.input.clone();
            tx_env.chain_id = tx.chain_id;
            tx_env.nonce = Some(tx.nonce);
            tx_env.access_list.clear();
            tx_env.blob_hashes.clear();
            tx_env.max_fee_per_blob_gas.take();
        }
        Transaction::Eip2930(tx) => {
            tx_env.gas_limit = tx.gas_limit;
            tx_env.gas_price = U256::from(tx.gas_price);
            tx_env.gas_priority_fee = None;
            tx_env.transact_to = match tx.to {
                TxKind::Call(to) => TransactTo::Call(to),
                TxKind::Create => TransactTo::Create,
            };
            tx_env.value = tx.value;
            tx_env.data = tx.input.clone();
            tx_env.chain_id = Some(tx.chain_id);
            tx_env.nonce = Some(tx.nonce);
            tx_env.access_list = tx
                .access_list
                .0
                .iter()
                .map(|l| {
                    (
                        l.address,
                        l.storage_keys
                            .iter()
                            .map(|k| U256::from_be_bytes(k.0))
                            .collect(),
                    )
                })
                .collect();
            tx_env.blob_hashes.clear();
            tx_env.max_fee_per_blob_gas.take();
        }
        Transaction::Eip1559(tx) => {
            tx_env.gas_limit = tx.gas_limit;
            tx_env.gas_price = U256::from(tx.max_fee_per_gas);
            tx_env.gas_priority_fee = Some(U256::from(tx.max_priority_fee_per_gas));
            tx_env.transact_to = match tx.to {
                TxKind::Call(to) => TransactTo::Call(to),
                TxKind::Create => TransactTo::Create,
            };
            tx_env.value = tx.value;
            tx_env.data = tx.input.clone();
            tx_env.chain_id = Some(tx.chain_id);
            tx_env.nonce = Some(tx.nonce);
            tx_env.access_list = tx
                .access_list
                .0
                .iter()
                .map(|l| {
                    (
                        l.address,
                        l.storage_keys
                            .iter()
                            .map(|k| U256::from_be_bytes(k.0))
                            .collect(),
                    )
                })
                .collect();
            tx_env.blob_hashes.clear();
            tx_env.max_fee_per_blob_gas.take();
        }
        Transaction::Eip4844(tx) => {
            tx_env.gas_limit = tx.gas_limit;
            tx_env.gas_price = U256::from(tx.max_fee_per_gas);
            tx_env.gas_priority_fee = Some(U256::from(tx.max_priority_fee_per_gas));
            tx_env.transact_to = TransactTo::Call(tx.to);
            tx_env.value = tx.value;
            tx_env.data = tx.input.clone();
            tx_env.chain_id = Some(tx.chain_id);
            tx_env.nonce = Some(tx.nonce);
            tx_env.access_list = tx
                .access_list
                .0
                .iter()
                .map(|l| {
                    (
                        l.address,
                        l.storage_keys
                            .iter()
                            .map(|k| U256::from_be_bytes(k.0))
                            .collect(),
                    )
                })
                .collect();
            tx_env.blob_hashes.clone_from(&tx.blob_versioned_hashes);
            tx_env.max_fee_per_blob_gas = Some(U256::from(tx.max_fee_per_blob_gas));
        }
    }
    tx_env
}

pub fn bytecode_to_evmcode(code: Bytecode) -> EvmCode {
    // convert code to LegacyAnalyzed if it is LegacyRaw by using to_analysed
    let code = match code {
        Bytecode::LegacyAnalyzed(_) => code,
        Bytecode::LegacyRaw(_) => to_analysed(code),
        _ => unreachable!(),
    };

    match code {
        Bytecode::LegacyAnalyzed(code) => EvmCode {
            bytecode: code.bytecode().clone(),
            original_len: code.original_len(),
            jump_table: code.jump_table().clone().0,
        },
        _ => unreachable!(),
    }
}

pub fn to_analysed(bytecode: Bytecode) -> Bytecode {
    let (bytes, len) = match bytecode {
        Bytecode::LegacyRaw(bytecode) => {
            let len = bytecode.len();
            let mut padded_bytecode = Vec::with_capacity(len + 33);
            padded_bytecode.extend_from_slice(&bytecode);
            padded_bytecode.resize(len + 33, 0);
            (Bytes::from(padded_bytecode), len)
        }
        n => return n,
    };
    let jump_table = analyze(bytes.as_ref());

    Bytecode::LegacyAnalyzed(LegacyAnalyzedBytecode::new(bytes, len, jump_table))
}

/// Analyze bytecode to build a jump map.
fn analyze(code: &[u8]) -> JumpTable {
    let mut jumps: BitVec<u8> = bitvec![u8, Lsb0; 0; code.len()];

    let range = code.as_ptr_range();
    let start = range.start;
    let mut iterator = start;
    let end = range.end;
    while iterator < end {
        let opcode = unsafe { *iterator };
        if opcode::JUMPDEST == opcode {
            // SAFETY: jumps are max length of the code
            unsafe { jumps.set_unchecked(iterator.offset_from(start) as usize, true) }
            iterator = unsafe { iterator.offset(1) };
        } else {
            let push_offset = opcode.wrapping_sub(opcode::PUSH1);
            if push_offset < 32 {
                // SAFETY: iterator access range is checked in the while loop
                iterator = unsafe { iterator.offset((push_offset + 2) as isize) };
            } else {
                // SAFETY: iterator access range is checked in the while loop
                iterator = unsafe { iterator.offset(1) };
            }
        }
    }

    JumpTable(Arc::new(jumps))
}
