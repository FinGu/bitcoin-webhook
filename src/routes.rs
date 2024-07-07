use std::{str::FromStr, sync::Arc};

use axum::{extract::State, Json};

use bitcoincore_rpc::{
    bitcoin::{Address, Amount, Denomination, Network},
    jsonrpc::serde::Deserialize,
    RpcApi,
};
use reqwest::StatusCode;
use serde_json::Value;
use tokio::{sync::Mutex, task};

use crate::{
    funcs::{self, get_time, WaitOnParams},
    Service,
};

pub async fn main() -> &'static str {
    "Hello World"
}

pub async fn test_webhook(Json(data): Json<Value>) -> &'static str {
    println!("{}", data);

    "Test webhook"
}

#[derive(Deserialize)]
pub struct WaitOnForm {
    address: Option<String>,
    amount_in_btc: String,
    confirmations_num: i32,
    expiry_in_mins: u64,
}

pub async fn wait_on(
    State(service): State<Arc<Mutex<Service>>>,
    Json(data): Json<WaitOnForm>,
) -> Result<String, StatusCode> {
    let WaitOnForm {
        address,
        amount_in_btc,
        confirmations_num,
        expiry_in_mins,
    } = data;

    let addr = match address {
        Some(a) => a,
        None => return Err(StatusCode::BAD_REQUEST),
    };

    let expiry_time = get_time() + (expiry_in_mins * 60);

    let amount = Amount::from_str_in(&amount_in_btc, Denomination::Bitcoin)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    if cfg!(debug_assertions) {
        Address::from_str(&addr)
            .map_err(|_| StatusCode::BAD_REQUEST)?
            .require_network(Network::Regtest)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    } else {
        Address::from_str(&addr)
            .map_err(|_| StatusCode::BAD_REQUEST)?
            .require_network(Network::Bitcoin)
            .map_err(|_| StatusCode::BAD_REQUEST)?;
    }

    task::spawn(funcs::wait_on(
        Arc::clone(&service),
        WaitOnParams {
            address: addr,
            amount,
            confirmations_num,
            timestamp: expiry_time,
        },
    ));

    Ok(String::from("Being waited on"))
}

pub async fn create_and_wait_on(
    State(service): State<Arc<Mutex<Service>>>,
    Json(data): Json<WaitOnForm>,
) -> Result<String, StatusCode> {
    let WaitOnForm {
        address: _,
        amount_in_btc,
        confirmations_num,
        expiry_in_mins,
    } = data;

    let address = {
        let sv = service.lock().await;
        sv.btc_client
            .get_new_address(None, None)
            .expect("Issue while generating a new address")
    }
    .assume_checked()
    .to_string();

    let expiry_time = get_time() + (expiry_in_mins * 60);

    let amount = Amount::from_str_in(&amount_in_btc, Denomination::Bitcoin)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    task::spawn(funcs::wait_on(
        Arc::clone(&service),
        WaitOnParams {
            address: address.clone(),
            amount,
            confirmations_num,
            timestamp: expiry_time,
        },
    ));

    Ok(address)
}
