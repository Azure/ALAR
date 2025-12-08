use anyhow::anyhow;
use log::error;
use std::cell::RefCell;
use std::env;
use std::sync;

static INIT: sync::Once = sync::Once::new();
thread_local!(static MY_ENDPOINT: RefCell<String> = const {RefCell::new(String::new())});
thread_local!(static MY_I_KEY: RefCell<String> = const {RefCell::new(String::new())});

fn parse_connection_string(cs: &str) -> anyhow::Result<(String, String)> {
    let mut ikey: Option<String> = None;
    let mut ingestion: Option<String> = None;

    for part in cs.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let mut kv = part.splitn(2, '=');
        let key = kv
            .next()
            .ok_or_else(|| anyhow!("Malformed connection string segment"))?
            .trim()
            .to_ascii_lowercase();
        let val = kv.next().unwrap_or("").trim().trim_matches('"');

        match key.as_str() {
            "instrumentationkey" | "ikey" => ikey = Some(val.to_string()),
            "ingestionendpoint" => ingestion = Some(val.to_string()),
            _ => { /* ignore other keys like LiveEndpoint, Authorization, EndpointSuffix */ }
        }
    }
    let ikey = ikey.ok_or_else(|| anyhow!("Connection string missing InstrumentationKey"))?;
    // If the connection string doesn’t include IngestionEndpoint, fallback to the legacy global endpoint.
    // (Using the regional IngestionEndpoint from the connection string is recommended.)
    // ref: Connection strings guidance
    let endpoint = ingestion.unwrap_or_else(|| "https://dc.services.visualstudio.com".to_string());
    Ok((endpoint, ikey))
}

fn teleendpoint_and_ikey() -> (String, String) {
    INIT.call_once(|| {
            // Before buidling the project, set the APPLICATIONINSIGHTS_CONNECTION_STRING environment variable.
            match parse_connection_string(env!("APPLICATIONINSIGHTS_CONNECTION_STRING")) {
                Ok((endpoint, ikey)) => {
                    MY_ENDPOINT.with(|e| {
                        *e.borrow_mut() = format!("{}/v2/track", endpoint.trim_end_matches('/'))
                    });
                    MY_I_KEY.with(|k| *k.borrow_mut() = ikey);
                }
                Err(e) => {
                    error!("Failed to parse APPLICATIONINSIGHTS_CONNECTION_STRING environment variable: {}", e);
                }
            }
    });
    (
        MY_ENDPOINT.with(|e| e.borrow().to_string()),
        MY_I_KEY.with(|k| k.borrow().to_string()),
    )
}

pub(crate) fn get_endpoint() -> String {
    let (endpoint, _) = teleendpoint_and_ikey();
    endpoint
}

pub(crate) fn get_ikey() -> String {
    let (_, ikey) = teleendpoint_and_ikey();
    ikey
}
