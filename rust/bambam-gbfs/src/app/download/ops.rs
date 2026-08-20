use reqwest::{Client, IntoUrl};
use serde::de::DeserializeOwned;

/// helper function for running a client HTTP GET call to retrieve a JSON object.
pub async fn retrieve_file<T: DeserializeOwned, U: IntoUrl>(
    client: &Client,
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

/// reads URLs from a CSV file at some column name into a vector.
pub fn gather_feeds(csv_file: &str, url_column: &str) -> Result<Vec<String>, String> {
    let mut reader = csv::ReaderBuilder::default()
        .has_headers(true)
        .from_path(csv_file)
        .map_err(|e| format!("failed to open CSV '{csv_file}': {e}"))?;
    let headers = reader
        .headers()
        .map_err(|e| format!("file '{csv_file}' failed to read headers: {e}"))?;
    let (col_idx, _) = headers
        .iter()
        .enumerate()
        .find(|(_, name)| *name == url_column)
        .ok_or_else(|| format!("column '{url_column}' not found"))?;

    let mut urls = vec![];
    for (idx, row_result) in reader.into_records().enumerate() {
        let row = row_result.map_err(|e| format!("failed to read CSV row {idx}: {e}"))?;
        let url = row
            .get(col_idx)
            .ok_or_else(|| format!("CSV row {idx} missing col {col_idx}"))?;
        urls.push(url.to_string());
    }
    Ok(urls)
}
