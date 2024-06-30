use pevm::{EvmCode, TransactTo};
use reth_primitives::revm_primitives::Bytecode;
use reth_primitives::{TransactionSigned, TxType, U256};

pub fn get_tx_env(tx_signed: TransactionSigned) -> pevm::TxEnv {
    let transaction = &tx_signed.transaction;
    let base_fee = Some(0);

    let caller = tx_signed.recover_signer().unwrap();
    let transact_to = match tx_signed.kind() {
        reth_primitives::TxKind::Call(addr) => TransactTo::Call(addr),
        reth_primitives::TxKind::Create => TransactTo::Create,
    };
    let gas_limit = tx_signed.gas_limit();
    let value = tx_signed.value();
    let data = tx_signed.input().clone();
    let nonce = Some(tx_signed.nonce());
    let chain_id = tx_signed.chain_id();
    let gas_price = tx_signed.effective_gas_price(base_fee);

    match transaction.tx_type() {
        TxType::Legacy => {
            // let tx = transaction.as_legacy().unwrap();
            pevm::TxEnv {
                caller,
                gas_limit,
                gas_price: U256::from(gas_price),
                transact_to,
                value,
                data,
                nonce,
                chain_id,
                ..Default::default()
            }
        }
        TxType::Eip2930 => {
            let tx = transaction.as_eip2930().unwrap();
            pevm::TxEnv {
                caller,
                gas_limit,
                gas_price: U256::from(gas_price),
                transact_to,
                value,
                data,
                nonce,
                chain_id,
                access_list: tx
                    .access_list
                    .0
                    .iter()
                    .map(|access| {
                        (
                            access.address,
                            access
                                .storage_keys
                                .iter()
                                .map(|&k| U256::from_be_bytes(*k))
                                .collect(),
                        )
                    })
                    .collect(),
                ..Default::default()
            }
        }
        TxType::Eip1559 => {
            let tx = transaction.as_eip1559().unwrap();
            pevm::TxEnv {
                caller,
                gas_limit,
                gas_price: U256::from(gas_price),
                transact_to,
                value,
                data,
                nonce,
                chain_id,
                access_list: tx
                    .access_list
                    .0
                    .iter()
                    .map(|access| {
                        (
                            access.address,
                            access
                                .storage_keys
                                .iter()
                                .map(|&k| U256::from_be_bytes(*k))
                                .collect(),
                        )
                    })
                    .collect(),
                gas_priority_fee: Some(U256::from(tx.max_priority_fee_per_gas)),
                ..Default::default()
            }
        }
        TxType::Eip4844 => {
            let tx = transaction.as_eip4844().unwrap();
            pevm::TxEnv {
                caller,
                gas_limit,
                gas_price: U256::from(gas_price),
                transact_to,
                value,
                data,
                nonce,
                chain_id,
                access_list: tx
                    .access_list
                    .0
                    .iter()
                    .map(|access| {
                        (
                            access.address,
                            access
                                .storage_keys
                                .iter()
                                .map(|&k| U256::from_be_bytes(*k))
                                .collect(),
                        )
                    })
                    .collect(),
                gas_priority_fee: Some(U256::from(tx.max_priority_fee_per_gas)),
                blob_hashes: tx.blob_versioned_hashes.clone(),
                max_fee_per_blob_gas: Some(U256::from(tx.max_fee_per_blob_gas)),
            }
        }
    }
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
