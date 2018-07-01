#![allow(dead_code)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate gotham_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate askama;
extern crate pretty_env_logger;
extern crate serde;
extern crate serde_json;
extern crate futures;
extern crate tokio_core;
extern crate tokio_timer;
#[macro_use] extern crate hyper;
extern crate hyper_tls;
extern crate bigdecimal;
extern crate hmac;
extern crate sha2;
extern crate gotham;
extern crate mime;
extern crate harsh;
extern crate qrcode;
extern crate base64;

mod coinmotion;
mod cache;
mod worker;
mod middleware;
mod db;
mod web;

use std::str::FromStr;
use std::sync::Arc;
use bigdecimal::BigDecimal;

use cache::Caches;

const COINMOTION_API_KEY: &str = "COINMOTION-API-KEY";
const COINMOTION_API_SECRET: &str = "COINMOTION-SECRET";
const HASHIDS_SALT: &str = "RANDOM-SALT";
const HTTP_PORT: u32 = 11133;

fn make_db() -> db::Database {
    db::Database::new(vec![
        db::Charge{
            id: 1,
            invoice_id: "Donate 100€".to_string(),
            eur_amount: BigDecimal::from_str("100").unwrap(),
            btc_address: "1Archive1n2C579dMsAu3iC6tWzuQJz8dN".to_string(),
        },
    ])
}

fn main() {
    pretty_env_logger::init();

    let db = Arc::new(make_db());
    let caches = Arc::new(Caches::new());
    let hashids = harsh::HarshBuilder::new()
        .salt(HASHIDS_SALT)
        .length(6)
        .init().unwrap();

    for c in db.charges().iter() {
        let hid = hashids.encode(&[c.id]).expect("valid hashid for charge");
        info!("Serving {} ({} EUR) at /{}", c.invoice_id, c.eur_amount, hid);
    }

    info!("Initialising task worker...");
    if !worker::start(caches.clone()) {
        error!("Failed to initialise the task worker!");
        return;
    }

    let addr = format!("127.0.0.1:{}", HTTP_PORT);
    gotham::start(addr, web::router(db, caches, hashids))
}

