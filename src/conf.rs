use std::fs::File;
use std::io::Read;
use toml;

use db;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub web: WebConfig,
    pub coinmotion: CoinMotionConfig,
    pub charges: Vec<db::Charge>,
}

#[derive(Debug, Deserialize)]
pub struct WebConfig {
    pub http_port: u32,
    pub hashids_salt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CoinMotionConfig {
    pub api_key: String,
    pub api_secret: String,
}

pub fn load() -> Config {
    let mut f = File::open("bitcharge.toml").expect("config file doesn't exists");
    let mut buf = vec![];
    f.read_to_end(&mut buf).expect("config file isn't readable");

    toml::from_slice(&buf).expect("config file isn't valid")
}
