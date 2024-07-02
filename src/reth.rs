use log::info;
use reth_evm::ConfigureEvm;
use reth_node_ethereum::EthEvmConfig;
use reth_primitives::{revm::config::revm_spec_by_timestamp_after_merge, TransactionSigned, U256};
use reth_provider::{BlockReaderIdExt, StateProviderFactory};
use reth_revm::{
    database::StateProviderDatabase,
    db::CacheDB,
    interpreter::gas::ZERO,
    primitives::{
        BlobExcessGasAndPrice, BlockEnv, CfgEnv, CfgEnvWithHandlerCfg, EVMError, EnvWithHandlerCfg,
        ExecutionResult, ResultAndState, SpecId,
    },
    DatabaseCommit,
};
use std::sync::Arc;

use crate::{
    lighthouse::BeaconEventsConfig,
    reth_db::reth_db_provider,
    utils::{chain_spec, get_tx_env_reth},
};

pub async fn execute_reth(txs: Vec<TransactionSigned>) -> Vec<ExecutionResult> {
    let provider = reth_db_provider();

    let beacon_client = BeaconEventsConfig::new();
    let payload_attributes = beacon_client.run().await.unwrap().data.payload_attributes;

    let chain_spec = chain_spec();

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
    let mut db = CacheDB::new(Arc::clone(&state));

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

    let block_env = BlockEnv {
        number: U256::from(latest_block_header.number + 1),
        timestamp: U256::from(payload_attributes.timestamp),
        coinbase: payload_attributes.suggested_fee_recipient,
        gas_limit: U256::from(latest_block_header.gas_limit),
        basefee: base_fee.map(U256::from).unwrap_or_default(),
        difficulty: U256::from(ZERO),
        prevrandao: Some(payload_attributes.prev_randao),
        blob_excess_gas_and_price,
    };

    let mut execution_result = Vec::new();
    info!("total txs: {:?}", txs.len());

    for tx in txs {
        let cfg = CfgEnv::default().with_chain_id(chain_spec.chain().id());
        let cfgenvwithhandlercfg = CfgEnvWithHandlerCfg::new_with_spec_id(cfg, spec_id);

        let env = EnvWithHandlerCfg::new_with_cfg_env(
            cfgenvwithhandlercfg,
            block_env.clone(),
            get_tx_env_reth(tx),
        );

        let evm_config = EthEvmConfig::default();

        // Configure the environment for the block.
        let mut evm = evm_config.evm_with_env(&mut db, env);

        let ResultAndState { result, state } = match evm.transact() {
            Ok(result) => result,
            Err(e) => {
                info!("Error executing transaction: {:?}", e);
                continue;
            }
        };
        drop(evm);
        db.commit(state);
        execution_result.push(result);
    }
    execution_result
}
