use std::fmt::Display;
use std::time::{SystemTime, UNIX_EPOCH};
use bigdecimal::BigDecimal;
use serde::de::{self, Deserialize, DeserializeOwned, Deserializer};
use serde::{Serialize, Serializer};
use serde_json;
use hyper::{self, Method, Request};
use hyper::header::ContentType;
use futures::{Future, Stream};
use hyper_tls::HttpsConnector;
use sha2::Sha512;
use hmac::{Hmac, Mac};
use mime;

use de::deserialize_big_decimal;

type Client = hyper::Client<HttpsConnector<hyper::client::HttpConnector>, hyper::Body>;

header! { (XCoinmotionAPIKey, "X-Coinmotion-APIKey") => [String] }
header! { (XCoinmotionSignature, "X-Coinmotion-Signature") => [String] }

/// Coinmotion withdrawal fee in EUR
pub const WITHDRAWAL_FEE: &str = "0.90";

pub struct Coinmotion<'a> {
    base_url: &'a str,
    api_key: &'a str,
    api_secret: &'a str,
    client: &'a Client,
}

impl<'a> Coinmotion<'a> {
    pub fn new(client: &'a Client, api_key: &'a str, api_secret: &'a str) -> Self {
        Self{
            base_url: "https://api.coinmotion.com/v1",
            api_key,
            api_secret,
            client,
        }
    }

    fn request<R>(&self, endpoint: &'static str, req: Request<hyper::Body>) -> impl Future<Item=R, Error=Error>
        where R: DeserializeOwned
    {
        self.client.request(req)
            .map_err(Error::ConnectionError)
            .and_then(move |res| {
                trace!("Coinmotion API [{}] response {}", endpoint, res.status());

                res.body().concat2()
                    .map_err(Error::ConnectionError)
                    .and_then(|body| {
                        serde_json::from_slice::<ResponseWrapper<R>>(&body)
                            .map_err(Error::ParseError)
                })
            })
            .and_then(|resp| {
                match resp {
                ResponseWrapper::Response(r) => Ok(r),
                ResponseWrapper::Error(err) => Err(Error::BackendError(err)),
                ResponseWrapper::UnknownStatus(s) => Err(Error::UnknownStatus(s)),
                }
            })
    }

    fn post<R, P>(&self, endpoint: &'static str, request: P) -> impl Future<Item=R, Error=Error>
        where P: Serialize,
              R: DeserializeOwned
    {
        let url = format!("{}{}", self.base_url, endpoint);
        let request = RequestWrapper{
            nonce: next_nonce(),
            request,
        };
        let request = serde_json::to_string(&request).unwrap();

        let mut mac = Hmac::<Sha512>::new_varkey(self.api_secret.as_bytes()).unwrap();
        mac.input(request.as_bytes());
        let sig = format!("{:x}", mac.result().code());

        let mut req = Request::new(Method::Post, url.parse().unwrap());
        req.headers_mut().set(ContentType(mime::APPLICATION_JSON));
        req.headers_mut().set(XCoinmotionAPIKey(self.api_key.to_string()));
        req.headers_mut().set(XCoinmotionSignature(sig));
        req.set_body(request);

        self.request(endpoint, req)
    }

    fn get<R>(&self, endpoint: &'static str) -> impl Future<Item=R, Error=Error>
        where R: DeserializeOwned
    {
        let url = format!("{}{}", self.base_url, endpoint);
        let req = Request::new(Method::Get, url.parse().unwrap());

        self.request(endpoint, req)
    }

    pub fn rates(&self) -> impl Future<Item=Rates, Error=Error> {
        self.get("/rates")
    }

    pub fn balances(&self) -> impl Future<Item=Balances, Error=Error> {
        self.post("/balances", BalancesRequest{})
    }

    pub fn sell(&self, amount: BuySellAmount) -> impl Future<Item=Trade, Error=Error> {
        self.post("/sell", SellRequest{
            amount_btc: if let BuySellAmount::BtcSatoshis(a) = amount {
                Some(a)
            } else {
                None
            },
            amount_cur: if let BuySellAmount::EurCents(a) = amount {
                Some(a)
            } else {
                None
            },
        })
        .map(|r| {
            debug!("Sell endpoint response: {:?}", r);
            r
        })
    }

    pub fn withdraw(&self, eur_cents: u64) -> impl Future<Item=Withdrawal, Error=Error> {
        self.post("/withdraw", WithdrawRequest{
            amount_cur: eur_cents,
        })
    }
}

#[derive(Debug)]
pub enum BuySellAmount {
    BtcSatoshis(u64),
    EurCents(u64),
}

fn next_nonce() -> u64 {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    now.as_secs() * 100 + now.subsec_nanos() as u64 / 10_000_000
}

#[derive(Debug)]
pub enum Error {
    ConnectionError(hyper::Error),
    ParseError(serde_json::Error),
    BackendError(String),
    UnknownStatus(String),
}

#[derive(Debug)]
enum ResponseWrapper<T> {
    Response(T),
    Error(String),
    UnknownStatus(String),
}

impl<'de, T> Deserialize<'de> for ResponseWrapper<T>
    where T: Deserialize<'de>
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where D: Deserializer<'de>
    {
        let mut map = serde_json::Map::deserialize(deserializer)?;

        let success = map.remove("success")
            .ok_or_else(|| de::Error::missing_field("success"))
            .map(Deserialize::deserialize)?
            .map_err(de::Error::custom)?;

        if success {
            map.remove("payload")
                .ok_or_else(|| de::Error::missing_field("payload"))
                .map(Deserialize::deserialize)?
                .map(ResponseWrapper::Response)
                .map_err(de::Error::custom)
        } else {
            let status = map.remove("status")
                .ok_or_else(|| de::Error::missing_field("status"))
                .map(Deserialize::deserialize)?
                .map_err(de::Error::custom)?;
            if status == "error" {
                map.remove("message")
                    .ok_or_else(|| de::Error::missing_field("message"))
                    .map(Deserialize::deserialize)?
                    .map(ResponseWrapper::Error)
                    .map_err(de::Error::custom)
            } else {
                Ok(ResponseWrapper::UnknownStatus(status))
            }
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
pub struct Rates {
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub btc_bid: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub btc_ask: BigDecimal,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Balances {
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub eur_bal: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub eur_avl: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub eur_res: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub btc_bal: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub btc_avl: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub btc_res: BigDecimal,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Trade {
    pub id: String,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub rate: BigDecimal,
    pub timestamp: String,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub amount_cur: BigDecimal,
    #[serde(deserialize_with = "deserialize_big_decimal")]
    pub amount_vir: BigDecimal,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Withdrawal {
    pub id: u64,
    pub iban: String,
    pub bic: String,
    pub ref_no: String,
}

#[derive(Serialize, Debug)]
struct BalancesRequest {
}

#[derive(Serialize, Debug)]
struct SellRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_btc: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    amount_cur: Option<u64>,
}

#[derive(Serialize, Debug)]
struct WithdrawRequest {
    amount_cur: u64,
}

#[derive(Serialize, Debug)]
struct RequestWrapper<T> {
    #[serde(serialize_with = "serialize_string")]
    nonce: u64,
    #[serde(flatten)]
    request: T,
}

fn serialize_string<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where T: Display,
          S: Serializer
{
    serializer.collect_str(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_trade() {
        let json = r#"
            {
                "id": "360001",
                "rate": "1234.56",
                "timestamp": "2018-07-06 21:04:54",
                "amount_cur": 1975.6,
                "amount_vir": -0.3575
            }
        "#;
        serde_json::from_str::<Trade>(json).unwrap();
    }

    #[test]
    fn deserialize_withdrawal() {
        let json = r#"
            {
                "id": 30001,
                "iban": "IBAN",
                "bic": "BIC",
                "ref_no": "X123"
            }
        "#;
        serde_json::from_str::<Withdrawal>(json).unwrap();
    }
}
