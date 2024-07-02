use std::path::Path;
use std::sync::Arc;

use crate::utils::chain_spec;
use reth_beacon_consensus::EthBeaconConsensus;
use reth_db::open_db_read_only;
use reth_db::DatabaseEnv;
use reth_node_ethereum::EthExecutorProvider;
use reth_provider::providers::BlockchainProvider;
use reth_provider::providers::StaticFileProvider;
use reth_provider::ProviderFactory;

use reth_blockchain_tree::BlockchainTree;
use reth_blockchain_tree::BlockchainTreeConfig;
use reth_blockchain_tree::ShareableBlockchainTree;
use reth_blockchain_tree::TreeExternals;

pub fn reth_db_provider() -> Arc<BlockchainProvider<Arc<DatabaseEnv>>> {
    let db_path = Path::new("/Users/chirag-bgh/Library/Application Support/reth/holesky/db");
    let db = Arc::new(open_db_read_only(db_path, Default::default()).unwrap());
    let chain_spec = chain_spec();

    let factory = ProviderFactory::new(
        db.clone(),
        chain_spec.clone(),
        StaticFileProvider::read_only(db_path.join("static_files")).unwrap(),
    );
    Arc::new({
        let consensus = Arc::new(EthBeaconConsensus::new(chain_spec.clone()));
        let executor = EthExecutorProvider::ethereum(chain_spec.clone());

        let tree_externals = TreeExternals::new(factory.clone(), consensus, executor);
        let tree =
            BlockchainTree::new(tree_externals, BlockchainTreeConfig::default(), None).unwrap();
        let blockchain_tree = Arc::new(ShareableBlockchainTree::new(tree));

        BlockchainProvider::new(factory, blockchain_tree).unwrap()
    })
}
