use reqwest::{Client, IntoUrl};
use serde::de::DeserializeOwned;

use crate::app::download::{EntryPoint, GbfsVersion};

/// helper function for running a client HTTP GET call to retrieve a JSON object.
pub async fn retrieve_file<'a, 'b, T: DeserializeOwned, U: IntoUrl>(
    client: &'b Client,
    url: U,
) -> Result<T, String> {
    let response = client
        .get(url)
        .header("User-Agent", "rust-reqwest")
        .send()
        .await
        .map_err(|e| format!("failed to connect to GBFS URL: {e}"))?;
    let status = response.status();
    if status.is_success() {
        let t: T = response.json().await.map_err(|e| {
            let type_name = std::any::type_name::<T>();
            format!("failed to deserialize {type_name} file from HTTP response: {e}")
        })?;
        Ok(t)
    } else {
        Err(format!("client response is {status}"))
    }
}
