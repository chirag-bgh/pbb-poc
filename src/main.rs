use log::info;
use pbb_poc::pbb::run_pevm;
use reth_primitives::TransactionSigned;
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    env_logger::init();

    info!("Starting PBB PoC");

    let txs = eth_get_best_transactions()
        .await
        .expect("Failed to send RPC request")
        .result;

    let pevm_result = run_pevm(txs).await;
    match pevm_result {
        Ok(_) => info!("PBB PoC completed successfully"),
        Err(e) => info!("PBB PoC failed: {:?}", e),
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcResponse<T> {
    jsonrpc: String,
    result: T,
    id: u32,
}

async fn eth_get_best_transactions(
) -> eyre::Result<RpcResponse<Vec<TransactionSigned>>, reqwest::Error> {
    let client = reqwest::Client::new();
    let res = client
        .post("http://localhost:8545/")
        .header("Content-Type", "application/json")
        .body(r#"{"jsonrpc":"2.0","method":"eth_getBestTransactions","params":[],"id":1}"#)
        .send()
        .await?;
    let rpc_response = res.json::<RpcResponse<Vec<TransactionSigned>>>().await?;
    Ok(rpc_response)
}
