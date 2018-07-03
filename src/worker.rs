use std::str::FromStr;
use std::sync::mpsc::sync_channel;
use std::sync::Arc;
use std::thread;
use std::time::{SystemTime, Duration};
use bigdecimal::{BigDecimal, Zero, One, ToPrimitive};
use hyper_tls::HttpsConnector;
use futures::{self, Future, Stream};
use tokio_core::reactor::Core;
use tokio_timer::Timer;
use hyper;

use conf::CoinmotionConfig;
use coinmotion::{Coinmotion, BuySellAmount, WITHDRAWAL_FEE};
use cache::Caches;

type Client = hyper::Client<HttpsConnector<hyper::client::HttpConnector>, hyper::Body>;

pub fn start(cm_conf: CoinmotionConfig, caches: Arc<Caches>) -> bool {
    let caches_outer = caches;
    let (tx, rx) = sync_channel(0);
    thread::spawn(move || {
        let mut core = Core::new().unwrap();
        let handle = &core.handle();

        let https = HttpsConnector::new(4, handle).unwrap();
        let client: Client = hyper::Client::configure()
            .keep_alive(false)
            .connector(https)
            .build(handle);

        let api = Coinmotion::new(
            &client,
            cm_conf.api_key.as_str(),
            cm_conf.api_secret.as_str(),
        );
        let api = &api;

        let mut cron = Scheduler::new(api, caches_outer.clone());
        let cron = &mut cron;

        core.run(futures::lazy(move || {
            let fut_update_rates = update_rates_task(api, caches_outer.clone());
            let fut_exchange = exchange_task(api);
            let fut_init = fut_update_rates.then(|_| fut_exchange);

            let timer = Timer::default();
            let fut_cron = timer.interval(Duration::from_secs(1))
                .map_err(|_| ())
                .for_each(move |_| cron.tick());

            fut_init
                // Unblock the main thread
                .then(move |r| {
                    tx.send(r.is_ok()).unwrap();
                    r
                })
                // Continue with cron
                .and_then(|_| fut_cron)
        })).unwrap();
    });

    rx.recv().unwrap_or(false)
}

const UPDATE_RATES_INTERVAL_SECS: u64 = 60;
const EXCHANGE_INTERVAL_SECS: u64 = 5 * 60;

struct Scheduler<'a> {
    api: &'a Coinmotion<'a>,
    caches: Arc<Caches>,
    update_rates_time: SystemTime,
    exchange_time: SystemTime,
}

impl<'a> Scheduler<'a> {
    fn new(api: &'a Coinmotion<'a>, caches: Arc<Caches>) -> Self {
        let now = SystemTime::now();
        Self{
            api,
            caches,
            update_rates_time: now + Duration::from_secs(UPDATE_RATES_INTERVAL_SECS),
            exchange_time: now + Duration::from_secs(EXCHANGE_INTERVAL_SECS),
        }
    }

    fn tick(&mut self) -> impl Future<Item=(), Error=()> + 'a {
        let now = SystemTime::now();

        let run_update_rates = self.update_rates_time <= now;
        let run_exchange = self.exchange_time <= now;

        if run_update_rates {
            self.update_rates_time = now + Duration::from_secs(UPDATE_RATES_INTERVAL_SECS);
        }
        if run_exchange {
            self.exchange_time = now + Duration::from_secs(EXCHANGE_INTERVAL_SECS);
        }

        let fut_tasks = if run_update_rates {
            let fut = update_rates_task(self.api, self.caches.clone());
            box_task(fut)
        } else {
            box_task(noop_task())
        };

        let fut_tasks = if run_exchange {
            let fut = exchange_task(self.api);
            box_task(fut_tasks.then(|_| fut))
        } else {
            fut_tasks
        };

        // Map any error from the tasks to a successful noop
        fut_tasks.or_else(|_| noop_task())
    }
}

fn box_task<'a, F>(fut: F) -> Box<Future<Item=(), Error=()> + 'a>
    where F: Future<Item=(), Error=()> + 'a
{
    Box::new(fut)
}

fn noop_task() -> impl Future<Item=(), Error=()> {
    futures::future::ok(())
}

fn update_rates_task(api: &Coinmotion, caches: Arc<Caches>) -> impl Future<Item=(), Error=()> {
    api.rates()
        .map(move |rates| {
            let mut rw_rates = caches.rates().write().unwrap();
            trace!("Updating cached rates - BTC bid: {} - BTC ask: {}", rates.btc_bid, rates.btc_ask);
            rw_rates.set(rates);
        })
        .map_err(|err| {
            error!("Failed to update rates cache: {:?}", err);
        })
}

fn exchange_task<'a>(api: &'a Coinmotion) -> impl Future<Item=(), Error=()> + 'a {
    let fut_balances = api.balances();

    fut_balances
        .map_err(|err| {
            error!("Failed to fetch balances: {:?}", err);
        })
        .and_then(move |bal| -> Box<Future<Item=(), Error=()> + 'a> {
            let one = BigDecimal::one().into_bigint_and_exponent().0;
            let withdrawal_fee = BigDecimal::from_str(WITHDRAWAL_FEE).unwrap();

            if !bal.btc_avl.is_zero() {
                // Always try to exchange BTC to EUR as quickly as possible
                let mul = BigDecimal::new(one, -8);
                let satoshis = (bal.btc_avl * mul).with_scale(0)
                    .to_u64().unwrap();
                Box::new(api.sell(BuySellAmount::BtcSatoshis(satoshis))
                    .map_err(|err| {
                        error!("Failed to sell BTC: {:?}", err);
                    })
                    .map(|_| ()))

            } else if bal.eur_avl > withdrawal_fee {
                // As a 2nd priority try to withdraw any balance we have
                let amount = bal.eur_avl - withdrawal_fee;
                let mul = BigDecimal::new(one, -2);
                let cents = (amount * mul).with_scale(0)
                    .to_u64().unwrap();
                Box::new(api.withdraw(cents)
                    .map_err(|err| {
                        error!("Failed to request withdrawal: {:?}", err);
                    })
                    .map(|_| ()))

            } else {
                // And there's nothing else to do
                Box::new(futures::future::ok(()))
            }
        })
}
