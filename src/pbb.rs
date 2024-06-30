#![allow(unused_imports)]

use anyhow::Chain;
use pevm::execute_revm;
use pevm::AccountBasic;
use pevm::BlobExcessGasAndPrice;
use pevm::EvmAccount;
use pevm::EvmCode;
use pevm::InMemoryStorage;
use pevm::PevmError;
use pevm::PevmResult;
use pevm::PevmUserType;
use pevm::Storage;
use pevm::TransactTo;
use pevm::CANCUN;
use reth_chainspec::ChainSpec;
use reth_primitives::revm::env::block_coinbase;
use reth_primitives::revm_primitives::EVMError;
use reth_primitives::transaction;
use reth_primitives::Address;
use reth_primitives::TransactionSigned;
use reth_primitives::U256;
use tracing::info;
use utils::bytecode_to_evmcode;

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
use reth::rpc::compat::block;
use reth::utils::db::open_db_read_only;
use reth_chainspec::ChainSpecBuilder;
use reth_chainspec::HOLESKY;
use reth_node_ethereum::EthExecutorProvider;

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
        .map(|tx_signed| utils::get_tx_env(tx_signed))
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

mod utils {
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
}
