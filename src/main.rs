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

use std::str::FromStr;
use std::sync::Arc;
use hyper::{Response, StatusCode};
use askama::Template;
use bigdecimal::{BigDecimal, ToPrimitive, One};
use gotham::http::response::create_response;
use gotham::router::Router;
use gotham::router::builder::*;
use gotham::state::{FromState, State};
use gotham::pipeline::new_pipeline;
use gotham::pipeline::single::single_pipeline;
use qrcode::{QrCode};
use qrcode::render::svg;

use cache::Caches;
use middleware::{Env, EnvMiddleware};

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
    gotham::start(addr, router(db, caches, hashids))
}

fn router(db: Arc<db::Database>, caches: Arc<Caches>, hashids: harsh::Harsh) -> Router {
    let (chain, pipeline) = single_pipeline(new_pipeline()
        .add(EnvMiddleware{
            db,
            caches,
            hashids,
        })
        .build());

    build_router(chain, pipeline, |route| {
        // Harsh currently panics on invalid alphabet input, so work-around it by only accepting
        // valid alphabet in the charge_id path component
        route.get_or_head("/:charge_id:[abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890]+")
            .with_path_extractor::<PayNowPath>()
            .to(get_pay_now_page);
    })
}

#[derive(Deserialize, StateData, StaticResponseExtender)]
struct PayNowPath {
    charge_id: String,
}

#[derive(Template)]
#[template(path = "pay_now.html")]
struct PayNowTemplate<'a> {
    invoice_id: &'a str,
    btc_address: &'a str,
    btc_amount: String,
    btc_link: String,
    qr_code_uri: String,
}

/// Calculate the foreign currency amount that as few digits as possible
/// while accepting a loss of up to 1 local unit.
fn local_to_pretty_foreign(local_amount: BigDecimal, foreign_bid: BigDecimal) -> BigDecimal {
    //    trunc((amount/bid) / 10^floor(log(10, 1/bid))) * 10^floor(log(10, 1/bid))
    // => trunc((amount/bid) * 10^(-floor(-log(10, bid)))) * 10^floor(-log(10, bid))
    // => trunc((amount/bid) * 10^(-exp)) * 10^exp
    let exp = (-foreign_bid.to_f64().unwrap().log10()).floor() as i64;
    let one = BigDecimal::one().into_bigint_and_exponent().0;
    // minus_exp = 10^-exp
    let minus_exp = BigDecimal::new(one.clone(), exp);
    // plus_exp = 10^exp
    let plus_exp = BigDecimal::new(one, -exp);
    ((local_amount / foreign_bid) * minus_exp).with_scale(0) * plus_exp
}

fn qr_code_uri(data: &[u8]) -> String {
    let qr = QrCode::with_error_correction_level(
        data,
        qrcode::EcLevel::H).expect("Usable QR code");

    let im = qr.render()
        .min_dimensions(200, 200)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#FFFFFF"))
        .build();

    let b64 = base64::encode(im.as_bytes());
    format!("data:image/svg+xml;base64,{}", b64)
}

fn get_pay_now_page(state: State) -> (State, Response) {
    let res = {
        let env = Env::borrow_from(&state);
        let path = PayNowPath::borrow_from(&state);

        let charge = env.hashids.decode(&path.charge_id)
            .and_then(|s| s.first().map(|i| *i))
            .and_then(|id| env.db.get_charge_by_id(id));

        if let Some(charge) = charge {
            let btc_bid = env.caches.rates().read().unwrap().get().btc_bid;
            let eur_amount = charge.eur_amount.clone();
            let btc_amount = local_to_pretty_foreign(eur_amount, btc_bid);

            let html = {
                let btc_address = charge.btc_address.as_str();
                let btc_amount = format!("{}", btc_amount);
                let btc_link = format!("bitcoin:{}?amount={}", btc_address, btc_amount);
                let qr_code_uri = qr_code_uri(btc_link.as_bytes());

                PayNowTemplate{
                    invoice_id: charge.invoice_id.as_str(),
                    btc_address,
                    btc_amount,
                    btc_link,
                    qr_code_uri,
                }.render().unwrap()
            };
            create_response(
                &state,
                StatusCode::Ok,
                Some((
                    html.into_bytes(),
                    mime::TEXT_HTML,
                )),
            )
        } else {
            create_response(
                &state,
                StatusCode::NotFound,
                None,
            )
        }
    };

    (state, res)
}
