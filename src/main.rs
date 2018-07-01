#![allow(dead_code)]

#[macro_use] extern crate serde_derive;
#[macro_use] extern crate gotham_derive;
#[macro_use] extern crate log;
#[macro_use] extern crate askama;
extern crate pretty_env_logger;
extern crate serde;
extern crate serde_json;
extern crate toml;
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

mod de;
mod conf;
mod coinmotion;
mod cache;
mod worker;
mod middleware;
mod db;
mod web;

use std::sync::Arc;

use cache::Caches;

fn main() {
    pretty_env_logger::init();

    let conf = conf::load();
    let db = Arc::new(db::Database::new(conf.charges));
    let caches = Arc::new(Caches::new());
    let hashids = harsh::HarshBuilder::new()
        .salt(conf.web.hashids_salt)
        .length(6)
        .init().unwrap();

    for c in db.charges().iter() {
        let hid = hashids.encode(&[c.id]).expect("valid hashid for charge");
        info!("Serving {} ({} EUR) at /{}", c.invoice_id, c.eur_amount, hid);
    }

    info!("Initialising task worker...");
    if !worker::start(conf.coinmotion, caches.clone()) {
        error!("Failed to initialise the task worker!");
        return;
    }

    let addr = format!("127.0.0.1:{}", conf.web.http_port);
    gotham::start(addr, web::router(db, caches, hashids))
}

