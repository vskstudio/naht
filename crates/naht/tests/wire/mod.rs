use naht_core::protocol::{self, Ack};
use reqwest::{Client, Response, StatusCode};
use serde::de::DeserializeOwned;

pub(crate) async fn decode<T: DeserializeOwned>(resp: Response) -> T {
    let bytes = resp.bytes().await.unwrap();
    protocol::from_msgpack(&bytes).unwrap()
}

pub(crate) async fn info(client: &Client, base: &str, place_id: Option<u64>) -> Response {
    let mut req = client.get(format!("{base}/info"));
    if let Some(id) = place_id {
        req = req.query(&[("place_id", id.to_string())]);
    }
    req.send().await.unwrap()
}

pub(crate) async fn heartbeat(client: &Client, base: &str) -> reqwest::Result<Response> {
    client.get(format!("{base}/heartbeat")).send().await
}

pub(crate) async fn ack(client: &Client, base: &str, paths: Vec<String>) -> StatusCode {
    let body = protocol::to_msgpack(&Ack { paths }).unwrap();
    client
        .post(format!("{base}/ack"))
        .body(body)
        .send()
        .await
        .unwrap()
        .status()
}
