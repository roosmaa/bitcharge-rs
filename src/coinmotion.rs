use std::fmt::Display;
use std::str::{FromStr};
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

type Client = hyper::Client<HttpsConnector<hyper::client::HttpConnector>, hyper::Body>;

header! { (XCoinMotionAPIKey, "X-CoinMotion-APIKey") => [String] }
header! { (XCoinMotionSignature, "X-CoinMotion-Signature") => [String] }

pub struct CoinMotion<'a> {
    base_url: &'a str,
    api_key: &'a str,
    api_secret: &'a str,
    client: &'a Client,
}

impl<'a> CoinMotion<'a> {
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
                trace!("CoinMotion API [{}] response {}", endpoint, res.status());

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
        req.headers_mut().set(XCoinMotionAPIKey(self.api_key.to_string()));
        req.headers_mut().set(XCoinMotionSignature(sig));
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
                map.remove("error")
                    .ok_or_else(|| de::Error::missing_field("error"))
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

#[derive(Serialize, Debug)]
struct BalancesRequest {
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

fn deserialize_big_decimal<'de, D>(deserializer: D) -> Result<BigDecimal, D::Error>
    where D: Deserializer<'de>
{
    let s: String = Deserialize::deserialize(deserializer)?;
    BigDecimal::from_str(s.as_str()).map_err(de::Error::custom)
}
