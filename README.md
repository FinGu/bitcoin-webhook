# bitcoin-webhook
Webhook like system ( a building block so to say ) to wait on a certain amount of BTC to be sent to an address.

### How it works:
For every request, it spawns a task that acts as a 'worker' for that certain address.<br/>
It uses the bitcoin rpc's function scantxoutset to grab the transactions with at least one confirmation that were sent to the address specified.<br/>
Those transactions are scanned and filtered with the confirmation number specified.<br/>
When a partial or successful payment happens, also when it's expired, the service sends a webhook to the link specified in the .env<br/>

### Pitfalls:
Doesn't have cold storage, if the daemon exits, all the running tasks will be forever lost. ( You need a sort of db )<br/>
Needs the confirmation number to be at least >= 1

### How to use:
Install rust and set up a working bitcoind<br/>
Compile with `cargo build`<br/>
Create a wallet that the system'll work with<br/>
Fill all the details in the .env file and you're good to go<br/>
<br/>
Available endpoints:<br/>
/wait_on - POST<br/> 
It will either return 'Being waited on' or fail with error 400 in case of bad input<br/>
The input:
```json
{
    "address": "bcrt1q4w5cypq4v0hl8g0mwda8hqkcelynvvds2sktj6",
    "amount_in_btc": "0.00005",
    "confirmations_num": 3,
    "expiry_in_mins": 60
}
```
/create_and_wait_on - POST<br/> 
It will either return the address that it's waiting on or fail with error 400 in case of bad input<br/>
The input:
```json
{
    "amount_in_btc": "0.00005",
    "confirmations_num": 3,
    "expiry_in_mins": 60
}
```

The webhook will be of the following syntax:<br/>
```json
{
    "address":"bcrt1qk2q65scedtnltrt0wqstzgjs20mx8778zefppv",
    "amount":2000000000,
    "confirmations_num":3,
    "expiry":1719779785,
    "required_amount":5000,
    "required_confirmations_num":3,
    "status":"Success"
}
```
For the example input of
```json
{
    "amount_in_btc": "0.00005",
    "confirmations_num": 3,
    "expiry_in_mins": 1
}
```
to /create_and_wait_on
