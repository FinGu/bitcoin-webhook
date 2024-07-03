use reqwest::RequestBuilder;
use serde::{Deserialize, Serialize};

use std::{
    error::Error,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use bitcoincore_rpc::{
    bitcoin::{Amount, SignedAmount},
    json::{ScanTxOutRequest, ScanTxOutResult, Utxo},
    Client, RpcApi,
};

use tokio::{sync::Mutex, time::sleep};

use crate::Service;

pub fn get_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug)]
pub enum WebhookError {
    RequestFailure,
    NotReachedYet,
    Expired,
    Completed,
    Btc(bitcoincore_rpc::Error),
}

type WebhookResult<T> = Result<T, WebhookError>;

impl std::fmt::Display for WebhookError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebhookError::RequestFailure => write!(f, "request failure"),
            WebhookError::Expired => write!(f, "transaction expired"),
            WebhookError::NotReachedYet => write!(f, "amount not reached yet"),
            WebhookError::Completed => write!(f, "completed the payment"),
            WebhookError::Btc(btc_err) => write!(f, "{}", btc_err),
        }
    }
}

impl From<bitcoincore_rpc::Error> for WebhookError {
    fn from(value: bitcoincore_rpc::Error) -> Self {
        Self::Btc(value)
    }
}

impl Error for WebhookError {}

#[derive(Serialize, Deserialize)]
pub struct WaitOnParams {
    pub address: String,
    pub amount: Amount,
    pub confirmations_num: i32,
    pub timestamp: u64,
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Success,
    PartialPayment,
    Expired,
    Waiting,
}

impl From<String> for Status {
    fn from(value: String) -> Self {
        match value.as_str() {
            "Success" => Self::Success,
            "Expired" => Self::Expired,
            "PartialPayment" => Self::PartialPayment,
            _ => Self::Waiting,
        }
    }
}

#[derive(Serialize)]
pub struct Webhook {
    #[serde(skip_serializing)]
    pub rq: RequestBuilder,

    pub expiry: u64,
    pub status: Status,
    pub address: String,

    pub required_amount: Amount,
    pub amount: Option<Amount>,
    pub required_confirmations_num: i32,
    pub confirmations_num: Option<i32>,
}

impl Webhook {
    pub fn new(params: WaitOnParams, rq: RequestBuilder) -> Self {
        Self {
            rq,
            expiry: params.timestamp,
            status: Status::Waiting,
            address: params.address.clone(),
            required_amount: params.amount,
            required_confirmations_num: params.confirmations_num,
            amount: None,
            confirmations_num: None,
        }
    }

    pub async fn send(&self) -> WebhookResult<()> {
        self.rq
            .try_clone()
            .unwrap()
            .json(self)
            .send()
            .await
            .map_err(|_| WebhookError::RequestFailure)?;

        Ok(())
    }

    pub async fn fill_and_send_partial(&mut self, response: &ScanTxOutResult) -> WebhookResult<()> {
        self.amount = Some(response.total_amount);

        if response.total_amount < self.required_amount {
            if response.total_amount > Amount::from_int_btc(0)
                && self.status != Status::PartialPayment
            {
                self.status = Status::PartialPayment;

                self.send().await?;
            }

            return Err(WebhookError::NotReachedYet);
        }

        Ok(())
    }
}

//returns total valid amnt
pub fn scan_utxo_transactions(
    client: &Client,
    utxo_list: &[Utxo],
    wb: &mut Webhook,
) -> (Amount, i32) {
    let transactions = utxo_list
        .iter()
        .filter_map(|each| client.get_transaction(&each.txid, None).ok())
        .collect::<Vec<_>>();

    let proper_transactions = transactions
        .into_iter()
        .filter(|each| each.info.confirmations >= wb.required_confirmations_num)
        .collect::<Vec<_>>();

    let len = proper_transactions.len() as i32;

    if len == 0 {
        return (Amount::from_int_btc(0), 0);
    }

    let amnt = proper_transactions
        .iter()
        .map(|each| each.amount)
        .sum::<SignedAmount>()
        .to_unsigned()
        .unwrap();

    let mut confirms = proper_transactions
        .iter()
        .map(|each| each.info.confirmations)
        .sum::<i32>();

    confirms /= proper_transactions.len() as i32;  // average of the confirms, doesn't really
                                                   // matter

    (amnt, confirms)
}

pub async fn wait_on_handle_scan(
    sv: &Service,
    webhook: &mut Webhook,
    scan_param: &[ScanTxOutRequest],
) -> WebhookResult<()> {
    if get_time() > webhook.expiry {
        webhook.status = Status::Expired;

        return Err(WebhookError::Expired);
    }

    let client = &sv.btc_client;

    let result = match client.scan_tx_out_set_blocking(scan_param) {
        Ok(o) => o,
        Err(e) => return Err(WebhookError::Btc(e)),
    };

    if webhook.fill_and_send_partial(&result).await.is_err() {
        //early return in case we didn't have transactions or it's partial
        return Ok(());
    }

    let (amnt, confirms) = scan_utxo_transactions(client, &result.unspents, webhook);

    if amnt >= webhook.required_amount {
        webhook.status = Status::Success;
        webhook.amount = Some(amnt);
        webhook.confirmations_num = Some(confirms);

        return Err(WebhookError::Completed);
    }

    Ok(())
}

pub async fn wait_on(service: Arc<Mutex<Service>>, params: WaitOnParams) -> WebhookResult<()> {
    let (wait_time, rq) = {
        let sv = service.lock().await;

        (
            sv.wait_time_in_seconds,
            sv.builded_request.try_clone().unwrap(),
        )
    };

    let scan_param = [ScanTxOutRequest::Single(format!(
        "addr({})",
        params.address.clone()
    ))];

    let mut partial_webhook = Webhook::new(params, rq);

    loop {
        let sv = service.lock().await;

        match wait_on_handle_scan(&sv, &mut partial_webhook, &scan_param).await {
            Ok(_) => {}
            Err(err) => {
                println!("{} {}", partial_webhook.address, err);
                partial_webhook.send().await?;
                break;
            }
        }

        drop(sv);
        sleep(Duration::from_secs(wait_time)).await;
    }

    Ok(())
}
