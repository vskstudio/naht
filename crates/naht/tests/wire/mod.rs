use naht_core::protocol::{self, Ack, CLIENT_HEADER, PLACE_ID_HEADER, TOKEN_HEADER};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;

fn plugin(request: RequestBuilder) -> RequestBuilder {
    request.header(CLIENT_HEADER, "plugin")
}

pub(crate) async fn decode<T: DeserializeOwned>(resp: Response) -> T {
    let bytes = resp.bytes().await.unwrap();
    protocol::from_msgpack(&bytes).unwrap()
}

pub(crate) async fn info(client: &Client, base: &str, place_id: Option<u64>) -> Response {
    let mut req = plugin(client.get(format!("{base}/info")));
    if let Some(id) = place_id {
        req = req.header(PLACE_ID_HEADER, id.to_string());
    }
    req.send().await.unwrap()
}

fn authorized(request: RequestBuilder, token: &str) -> RequestBuilder {
    plugin(request).header(TOKEN_HEADER, token)
}

pub(crate) async fn heartbeat(
    client: &Client,
    base: &str,
    token: &str,
) -> reqwest::Result<Response> {
    authorized(client.get(format!("{base}/heartbeat")), token)
        .send()
        .await
}

pub(crate) async fn ack(
    client: &Client,
    base: &str,
    session: &str,
    seqs: Vec<u64>,
    token: &str,
) -> StatusCode {
    let body = protocol::to_msgpack(&Ack { seqs }).unwrap();
    authorized(client.post(format!("{base}/ack?session={session}")), token)
        .body(body)
        .send()
        .await
        .unwrap()
        .status()
}
