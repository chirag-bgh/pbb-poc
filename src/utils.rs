use pevm::{EvmCode, TransactTo};
use reth_primitives::revm_primitives::Bytecode;
use reth_primitives::{Transaction, TransactionSigned, TxKind, U256};

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
    match code {
        Bytecode::LegacyAnalyzed(code) => EvmCode {
            bytecode: code.bytecode().clone(),
            original_len: code.original_len(),
            jump_table: code.jump_table().clone().0,
        },
        _ => unimplemented!(),
    }
}
