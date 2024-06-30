use pbb_poc::pbb::run;
use reth_primitives::TransactionSigned;
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    let txs = send_rpc_request()
        .await
        .expect("Failed to send RPC request")
        .result;
    run(txs);
}

#[derive(Serialize, Deserialize, Debug)]
struct RpcResponse<T> {
    jsonrpc: String,
    result: T,
    id: u32,
}

async fn send_rpc_request() -> eyre::Result<RpcResponse<Vec<TransactionSigned>>, reqwest::Error> {
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
