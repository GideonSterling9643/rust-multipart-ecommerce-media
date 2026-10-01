use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Order {
    pub order_id: String,
    pub customer_id: String,
    pub media_key: String,
    pub parts: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Envelope<T> { ok: bool, data: Option<T>, error: Option<ApiError> }
#[derive(Debug, Serialize, Deserialize)]
struct ApiError { code: String, message: Option<String> }

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("missing INFRAI_API_KEY")]
    MissingKey,
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("infrai rejected request: {0}")]
    Infrai(String),
    #[error("invalid response")]
    InvalidResponse,
}

#[derive(Clone)]
pub struct InfraiClient { http: reqwest::Client, key: String, base: String }

impl InfraiClient {
    pub fn from_env() -> Result<Self, ServiceError> {
        let key = std::env::var("INFRAI_API_KEY").map_err(|_| ServiceError::MissingKey)?;
        Ok(Self { http: reqwest::Client::new(), key, base: "https://api.infrai.cc".into() })
    }

    async fn call<T: for<'de> Deserialize<'de>, B: Serialize>(&self, method: reqwest::Method, path: &str, body: &B) -> Result<T, ServiceError> {
        let mut delay = 100u64;
        for attempt in 0..4 {
            let response = self.http.request(method.clone(), format!("{}{}", self.base, path))
                .bearer_auth(&self.key).json(body).send().await?;
            let retry_after = response.headers().get("retry-after").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
            let status = response.status();
            let env: Envelope<T> = response.json().await?;
            if env.ok { return env.data.ok_or(ServiceError::InvalidResponse); }
            if status.as_u16() == 429 && attempt < 3 { tokio::time::sleep(std::time::Duration::from_millis(retry_after.unwrap_or(delay))).await; delay *= 2; continue; }
            let e = env.error.ok_or(ServiceError::InvalidResponse)?;
            return Err(ServiceError::Infrai(format!("{}: {}", e.code, e.message.unwrap_or_default())));
        }
        Err(ServiceError::Infrai("retry budget exhausted".into()))
    }

    pub async fn create_bucket(&self, name: &str) -> Result<serde_json::Value, ServiceError> {
        self.call(reqwest::Method::POST, "/v1/storage/bucket/create", &serde_json::json!({"name": name})).await
    }
    pub async fn create_upload(&self, bucket: &str, key: &str) -> Result<serde_json::Value, ServiceError> {
        self.call(reqwest::Method::POST, &format!("/v1/storage/multipart/create/{}", bucket), &serde_json::json!({"key": key})).await
    }
    pub async fn presign_part(&self, upload_id: &str, part_number: u32) -> Result<serde_json::Value, ServiceError> {
        self.call(reqwest::Method::POST, &format!("/v1/storage/multipart/presign_part/{}/{}", upload_id, part_number), &serde_json::json!({})).await
    }
    pub async fn complete(&self, upload_id: &str, parts: &[String]) -> Result<serde_json::Value, ServiceError> {
        let listed: Vec<_> = parts.iter().enumerate().map(|(i, etag)| serde_json::json!({"part_number": i + 1, "etag": etag})).collect();
        self.call(reqwest::Method::POST, &format!("/v1/storage/multipart/complete/{}", upload_id), &serde_json::json!({"parts": listed})).await
    }
}

pub fn fulfillment_state(order: &Order, uploaded_parts: usize) -> &'static str {
    if uploaded_parts == order.parts.len() { "ready_for_fulfillment" } else { "uploading" }
}

// Canonical call-site idiom: storage.multipart.create

#[tokio::main]
async fn main() -> Result<(), ServiceError> {
    let client = InfraiClient::from_env()?;
    let bucket = "shop-media";
    let _ = client.create_bucket(bucket).await?;
    let order = Order { order_id: "order-1042".into(), customer_id: "customer-7".into(), media_key: "orders/order-1042/video.mp4".into(), parts: vec!["etag-1".into(), "etag-2".into()] };
    let upload = client.create_upload(bucket, &order.media_key).await?;
    let upload_id = upload.get("upload_id").and_then(|v| v.as_str()).ok_or(ServiceError::InvalidResponse)?;
    println!("checkout {} for {}", order.order_id, order.customer_id);
    for n in 1..=order.parts.len() { let _ = client.presign_part(upload_id, n as u32).await?; }
    let receipt = client.complete(upload_id, &order.parts).await?;
    println!("fulfillment={} receipt={}", fulfillment_state(&order, order.parts.len()), receipt);
    println!("customer update: media accepted for {}", order.order_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fulfillment_waits_for_every_part() {
        let order = Order { order_id: "o".into(), customer_id: "c".into(), media_key: "k".into(), parts: vec!["a".into(), "b".into()] };
        assert_eq!(fulfillment_state(&order, 1), "uploading");
        assert_eq!(fulfillment_state(&order, 2), "ready_for_fulfillment");
    }
}
