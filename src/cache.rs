use coinmotion;

use std::sync::RwLock;
use std::time::{Duration, SystemTime};

type RatesCache = ExpiringValueCache<coinmotion::Rates>;

pub struct Caches {
    rates: RwLock<RatesCache>,
}

impl Caches {
    pub fn new() -> Self {
        Self{
            rates: RwLock::new(RatesCache::new(
                Duration::from_secs(3600),
            )),
        }
    }

    pub fn rates(&self) -> &RwLock<RatesCache> {
        &self.rates
    }
}

pub struct ExpiringValueCache<T> {
    value: Option<T>,
    update_time: SystemTime,
    valid_for: Duration,
}

impl<T> ExpiringValueCache<T>
    where T: Clone
{
    fn new(valid_for: Duration) -> Self {
        Self{
            value: None,
            update_time: SystemTime::now(),
            valid_for,
        }
    }

    pub fn get(&self) -> T {
        match self.update_time.elapsed() {
            Ok(elapsed) => {
                if elapsed > self.valid_for {
                    panic!("Cached value too stale");
                }
            },
            Err(err) => warn!("Cache update timestamp time-travelled - {:?}", err),
        }

        let value = self.value.clone();
        value.expect("Value not cached yet")
    }

    pub fn set(&mut self, value: T) {
        self.update_time = SystemTime::now();
        self.value = Some(value);
    }
}

