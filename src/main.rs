use axum::{
    routing::{get, post},
    Router,
};
use bitcoincore_rpc::{Auth, RpcApi};
use dotenv::dotenv;
use reqwest::RequestBuilder;
use std::{
    env::{self},
    sync::Arc,
};
use tokio::sync::Mutex;

mod funcs;
mod routes;

pub struct Service {
    wait_time_in_seconds: u64,
    btc_client: bitcoincore_rpc::Client,
    builded_request: RequestBuilder,
}

fn extract(name: &[&str]) -> Vec<String> {
    name.iter()
        .map(|e| env::var(e).unwrap_or_else(|_| panic!("{} not in .env", e)))
        .collect::<Vec<_>>()
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let env_vars = extract(&[
        "CON_URL",
        "WALLET",
        "WEBHOOK",
        "RPC_USER_PASS",
        "WAIT_TIME_SECS",
        "PORT"
    ]);

    let [con_url, wallet, webhook, user_pass, wait_time_in_secs, port] =
        <[String; 6]>::try_from(env_vars).expect("Failed to read all env vars");

    let (user, pass) = user_pass.split_once(':').expect("Invalid user/pass syntax");

    let user = user.to_owned();
    let pass = pass.to_owned();

    let btc_client = bitcoincore_rpc::Client::new(&con_url, Auth::UserPass(user, pass))
        .expect("Failed to connect to bitcoin core");

    let _ = btc_client.unload_wallet(Some(&wallet));

    btc_client
        .load_wallet(&wallet)
        .expect("Failed to load wallet");

    let builded_request = reqwest::Client::new().post(webhook);

    let service = Arc::new(Mutex::new(Service {
        builded_request,
        btc_client,
        wait_time_in_seconds: wait_time_in_secs.parse().unwrap(),
    }));

    let app = Router::new()
        .route("/", get(routes::main))
        .route("/wait_on", post(routes::wait_on))
        .route("/create_and_wait_on", post(routes::create_and_wait_on))
        .route("/test_webhook", post(routes::test_webhook))
        .with_state(service);

    let listener = tokio::net::TcpListener::bind(&format!("0.0.0.0:{}", port)).await.unwrap();

    axum::serve(listener, app).await.unwrap();
}
