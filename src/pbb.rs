use pevm::execute_revm;
use pevm::AccountBasic;
use pevm::BlobExcessGasAndPrice;
use pevm::EvmAccount;
use pevm::InMemoryStorage;
use pevm::PevmUserType;
use pevm::CANCUN;
use reth_chainspec::ChainSpec;
use reth_primitives::revm::env::block_coinbase;
use reth_primitives::revm_primitives::EVMError;
use reth_primitives::Address;
use reth_primitives::TransactionSigned;
use reth_primitives::U256;
use tracing::info;

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::Path;
use std::sync::Arc;
use std::thread;

use reth::beacon_consensus::EthBeaconConsensus;
use reth::blockchain_tree::BlockchainTree;
use reth::blockchain_tree::BlockchainTreeConfig;
use reth::blockchain_tree::ShareableBlockchainTree;
use reth::blockchain_tree::TreeExternals;

use reth::providers::providers::BlockchainProvider;
use reth::providers::providers::StaticFileProvider;
use reth::providers::BlockReaderIdExt;
use reth::providers::ProviderFactory;
use reth::providers::StateProviderFactory;
use reth::revm::database::StateProviderDatabase;
use reth::revm::db::CacheDB;
use reth::revm::interpreter::gas::ZERO;
use reth::utils::db::open_db_read_only;
use reth_chainspec::HOLESKY;
use reth_node_ethereum::EthExecutorProvider;

use crate::utils::{bytecode_to_evmcode, get_tx_env};

pub fn run(txs_signed: Vec<TransactionSigned>) {
    let db_path = Path::new("/Users/chirag-bgh/Library/Application Support/reth/holesky/db");
    let db = Arc::new(open_db_read_only(db_path, Default::default()).unwrap());

    let chain_spec = Arc::new(ChainSpec {
        chain: HOLESKY.chain.clone(),
        genesis: HOLESKY.genesis.clone(),
        hardforks: HOLESKY.hardforks.clone(),
        genesis_hash: Some(HOLESKY.genesis_hash()),
        paris_block_and_final_difficulty: HOLESKY.paris_block_and_final_difficulty,
        deposit_contract: HOLESKY.deposit_contract.clone(),
        base_fee_params: HOLESKY.base_fee_params.clone(),
        prune_delete_limit: HOLESKY.prune_delete_limit,
    });
    let factory = ProviderFactory::new(
        db.clone(),
        chain_spec.clone(),
        StaticFileProvider::read_only(db_path.join("static_files")).unwrap(),
    );
    let provider = Arc::new({
        let consensus = Arc::new(EthBeaconConsensus::new(chain_spec.clone()));
        let executor = EthExecutorProvider::ethereum(chain_spec.clone());

        let tree_externals = TreeExternals::new(factory.clone(), consensus, executor);
        let tree =
            BlockchainTree::new(tree_externals, BlockchainTreeConfig::default(), None).unwrap();
        let blockchain_tree = Arc::new(ShareableBlockchainTree::new(tree));

        BlockchainProvider::new(factory, blockchain_tree).unwrap()
    });

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

    let blob_excess_gas_and_price =
        if let Some(execess_blog_gas) = latest_block_header.excess_blob_gas {
            Some(BlobExcessGasAndPrice::new(execess_blog_gas))
        } else {
            None
        };

    let block_env = pevm::BlockEnv {
        number: U256::from(latest_block_header.number),
        timestamp: U256::from(latest_block_header.timestamp),
        coinbase: block_coinbase(&chain_spec, latest_block_header.header(), true),
        gas_limit: U256::from(latest_block_header.gas_limit),
        basefee: U256::from(latest_block_header.base_fee_per_gas.unwrap_or_default()),
        difficulty: U256::from(ZERO),
        prevrandao: Some(latest_block_header.mix_hash),
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
        }
        Err(e) => {
            info!("Error executing txs: {:?}", e);
        }
    }
}
