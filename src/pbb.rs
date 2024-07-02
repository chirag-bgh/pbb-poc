use log::info;
use pevm::execute_revm;
use pevm::AccountBasic;
use pevm::BlobExcessGasAndPrice;
use pevm::EvmAccount;
use pevm::InMemoryStorage;
use pevm::PevmResult;
use pevm::PevmUserType;
use pevm::CANCUN;
use reth_primitives::revm::config::revm_spec_by_timestamp_after_merge;
use reth_primitives::revm_primitives::EVMError;
use reth_primitives::Address;
use reth_primitives::TransactionSigned;
use reth_primitives::U256;
use reth_revm::primitives::SpecId;

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::thread;

use reth_chainspec::HOLESKY;
use reth_provider::BlockReaderIdExt;
use reth_provider::StateProviderFactory;
use reth_revm::database::StateProviderDatabase;
use reth_revm::db::CacheDB;
use reth_revm::interpreter::gas::ZERO;

use crate::lighthouse::BeaconEventsConfig;
use crate::reth_db::reth_db_provider;
use crate::utils::chain_spec;
use crate::utils::{bytecode_to_evmcode, get_tx_env};

pub async fn run_pevm(txs_signed: Vec<TransactionSigned>) -> PevmResult {
    let provider = reth_db_provider();
    let chain_spec = chain_spec();

    let beacon_client = BeaconEventsConfig::new();
    let payload_attributes = beacon_client.run().await.unwrap().data.payload_attributes;

    let latest_block_header = provider
        .latest_header()
        .map_err(|_e| EVMError::Database(String::from("Error fetching latest sealed header")))
        .unwrap()
        .unwrap();
    let latest_state = provider
        .state_by_block_hash(latest_block_header.hash())
        .map_err(|_| EVMError::Database(String::from("Error fetching latest state")))
        .unwrap();
    let state = Arc::new(StateProviderDatabase::new(latest_state));
    let cache_db = CacheDB::new(Arc::clone(&state));

    let accounts: HashMap<Address, EvmAccount> = cache_db
        .accounts
        .into_iter()
        .map(|(addr, acc)| {
            (
                addr,
                EvmAccount {
                    basic: AccountBasic {
                        balance: acc.info.balance,
                        nonce: acc.info.nonce,
                        code_hash: Some(acc.info.code_hash),
                        code: Some(bytecode_to_evmcode(acc.info.code.unwrap())),
                    },
                    storage: acc.storage.into_iter().collect(),
                },
            )
        })
        .collect();
    let block_hashes: pevm::AHashMap<U256, reth_primitives::B256> =
        cache_db.block_hashes.into_iter().collect();
    let pevm_storage = InMemoryStorage::new(accounts, block_hashes);
    let holesky = HOLESKY.clone();

    let spec_id = revm_spec_by_timestamp_after_merge(&chain_spec, payload_attributes.timestamp);

    let base_fee = latest_block_header
        .header()
        .next_block_base_fee(chain_spec.base_fee_params_at_timestamp(payload_attributes.timestamp));

    let blob_excess_gas_and_price = latest_block_header
        .header()
        .next_block_excess_blob_gas()
        .or_else(|| {
            if spec_id == SpecId::CANCUN {
                // default excess blob gas is zero
                Some(0)
            } else {
                None
            }
        })
        .map(BlobExcessGasAndPrice::new);

    let block_env = pevm::BlockEnv {
        number: U256::from(latest_block_header.number + 1),
        timestamp: U256::from(payload_attributes.timestamp),
        coinbase: payload_attributes.suggested_fee_recipient,
        gas_limit: U256::from(latest_block_header.gas_limit),
        basefee: base_fee.map(U256::from).unwrap_or_default(),
        difficulty: U256::from(ZERO),
        prevrandao: Some(payload_attributes.prev_randao),
        blob_excess_gas_and_price,
    };

    let transactions_envs: Vec<pevm::TxEnv> = txs_signed
        .into_iter()
        .map(|tx_signed| get_tx_env(tx_signed))
        .collect();

    let concurrency_level = thread::available_parallelism().unwrap_or(NonZeroUsize::MIN);

    let pevm_result = execute_revm(
        pevm_storage,
        holesky.chain,
        CANCUN,
        block_env,
        transactions_envs,
        concurrency_level,
        PevmUserType::BlockBuilder,
    );

    match pevm_result {
        Ok(_) => {
            info!("txs executed successfully");
            pevm_result
        }
        Err(e) => {
            info!("Error executing txs: {:?}", e);
            Err(e)
        }
    }
}
