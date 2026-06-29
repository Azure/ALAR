// Copyright (c) Microsoft Corporation. All rights reserved.
// Licensed under the terms found in the LICENSE file in the root of this source tree.

use crate::cli;
use crate::distro;
use crate::helper;
use chrono::Utc;
use log::debug;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Serialize;
use std::env;
use std::collections::HashMap;
use std::time::Duration;

#[allow(dead_code)]
#[derive(Serialize, Debug, Copy, Clone, Eq, PartialEq)]
pub enum SeverityLevel {
    Verbose,
    Information,
    Warning,
    Error,
    Critical,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionBaseData {
    ver: u8,
    exceptions: Vec<Exceptions>,
    severity_level: SeverityLevel,
    properties: HashMap<String, String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TraceBaseData {
    ver: u8,
    message: String,
    severity_level: SeverityLevel,
    properties: HashMap<String, String>,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Exceptions {
    type_name: String,
    message: String,
    stack: String,
    has_full_stack: bool,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TraceBase {
    base_type: String,
    base_data: TraceBaseData,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionBase {
    base_type: String,
    base_data: ExceptionBaseData,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TraceEnvelope {
    name: String,
    time: String,
    #[serde(rename = "iKey")]
    i_key: String,
    tags: HashMap<String, String>,
    data: TraceBase,
}

#[derive(Serialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExceptionEnvelope {
    name: String,
    time: String,
    #[serde(rename = "iKey")]
    i_key: String,
    tags: HashMap<String, String>,
    data: ExceptionBase,
}

#[derive(Debug, Clone)]
pub struct OsNameArchitecture {
    repair_os_name: String,
    repair_os_version: String,
    arch: String,
}

impl OsNameArchitecture {
    fn new(architecture: distro::Architecture) -> Self {
        let repair_os_name = helper::get_repair_os_name().unwrap_or("Unknown".to_owned());
        let repair_os_version = helper::get_repair_os_version().unwrap_or("Unknown".to_owned());
        let arch = format!("{}", architecture);

        OsNameArchitecture {
            repair_os_name,
            repair_os_version,
            arch,
        }
    }
}

fn initiator_label(initiator: &cli::Initiator) -> &'static str {
    match initiator {
        cli::Initiator::Cli => "CLI",
        cli::Initiator::RecoverVm => "RecoverVm",
        cli::Initiator::SelfHelp => "SelfHelp",
    }
}

fn telemetry_properties(
    cli_info: &cli::CliInfo,
    distro: &distro::Distro,
    repair_info: &OsNameArchitecture,
) -> HashMap<String, String> {
    HashMap::from([
        (
            "Initiator".to_owned(),
            initiator_label(&cli_info.initiator).to_owned(),
        ),
        ("Action".to_owned(), cli_info.actions.clone()),
        ("Architecture".to_owned(), repair_info.arch.clone()),
        (
            "RepairDistroNameVersion".to_owned(),
            format!(
                "{} : {}",
                repair_info.repair_os_name, repair_info.repair_os_version
            ),
        ),
        (
            "RecoverDistroNameVersion".to_owned(),
            format!(
                "{} : {}",
                distro.distro_name_version.name, distro.distro_name_version.version_id
            ),
        ),
    ])
}

fn telemetry_tags(cli_info: &cli::CliInfo) -> HashMap<String, String> {
    HashMap::from([
        ("ai.cloud.role".to_owned(), "ALAR".to_owned()),
        (
            "ai.internal.sdkVersion".to_owned(),
            clap::crate_version!().to_owned(),
        ),
        (
            "Initiator".to_owned(),
            initiator_label(&cli_info.initiator).to_owned(),
        ),
        ("Action".to_owned(), cli_info.actions.clone()),
    ])
}

pub(crate) fn create_exception_envelope(
    severity_level: SeverityLevel,
    type_name: &str,
    message: &str,
    stack: &str,
    cli_info: &cli::CliInfo,
    distro: &distro::Distro,
) -> ExceptionEnvelope {
    let repair_info = OsNameArchitecture::new(distro.architecture);

    ExceptionEnvelope {
        name: "Microsoft.ApplicationInsights.Exception".to_owned(),
        time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        i_key: get_ikey(),
        tags: telemetry_tags(cli_info),
        data: ExceptionBase {
            base_type: "ExceptionData".to_owned(),
            base_data: ExceptionBaseData {
                ver: 2,
                exceptions: vec![Exceptions {
                    type_name: type_name.to_owned(),
                    message: message.to_owned(),
                    stack: stack.to_owned(),
                    has_full_stack: true,
                }],
                severity_level,
                properties: telemetry_properties(cli_info, distro, &repair_info),
            },
        },
    }
}

pub(crate) fn create_trace_envelope(
    severity_level: SeverityLevel,
    message: &str,
    cli_info: &cli::CliInfo,
    distro: &distro::Distro,
) -> TraceEnvelope {
    let repair_info = OsNameArchitecture::new(distro.architecture);

    TraceEnvelope {
        name: "Microsoft.ApplicationInsights.Message".to_owned(),
        time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        i_key: get_ikey(),
        tags: telemetry_tags(cli_info),
        data: TraceBase {
            base_type: "MessageData".to_owned(),
            base_data: TraceBaseData {
                ver: 2,
                message: message.to_owned(),
                severity_level,
                properties: telemetry_properties(cli_info, distro, &repair_info),
            },
        },
    }
}

// See the following doc about key information: https://learn.microsoft.com/en-us/azure/azure-monitor/app/connection-strings
const KEY_LOCATION : &str = "InstrumentationKey=67ca72ac-0de7-4f4d-b66a-e8af80638c00;IngestionEndpoint=https://polandcentral-0.in.applicationinsights.azure.com/;LiveEndpoint=https://polandcentral.livediagnostics.monitor.azure.com/;ApplicationId=ff9a02a2-91f0-4e98-96a5-06dfb4621f40";
fn connection_string_value<'a>(connection_string: &'a str, key: &str) -> Option<&'a str> {
    connection_string.split(';').find_map(|part| {
        let part = part.trim();
        let (part_key, value) = part.split_once('=')?;

        if part_key.eq_ignore_ascii_case(key) {
            Some(value.trim().trim_matches('"'))
        } else {
            None
        }
    })
}

pub(crate) fn get_endpoint() -> String {
    connection_string_value(KEY_LOCATION, "IngestionEndpoint")
        .map(|endpoint| format!("{endpoint}/v2/track"))
        .unwrap_or_else(|| "https://dc.services.visualstudio.com/v2/track".to_owned())
}   

pub(crate) fn get_ikey() -> String {
    connection_string_value(KEY_LOCATION, "InstrumentationKey")
        .unwrap_or_default()
        .to_owned()
}

pub(crate) fn send_envelope<T: Serialize>(envelope: &T) -> anyhow::Result<()> {
    if env::var("ALAR_TELEMETRY_DISABLED").is_ok() {
        debug!("Telemetry is disabled via ALAR_TELEMETRY_DISABLED environment variable. Skipping sending telemetry.");
        return Ok(());
    }
    let endpoint = get_endpoint();

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let client = Client::builder().timeout(Duration::from_secs(5)).build()?;
    match client
        .post(&endpoint)
        .headers(headers)
        .json(envelope)
        .send()
    {
        Ok(response) => {
            debug!(
                "Telemetry sent, status: {} and response: {}",
                response.status(),
                response.text().unwrap_or_default()
            );
            Ok(())
        }
        Err(e) => {
            // Silently catch the error
            debug!("Error sending telemetry data: {}", e);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli;
    use crate::distro;
    use env_logger::Env;

    fn test_create_trace_envelope() -> anyhow::Result<()> {
        let severity_level = SeverityLevel::Information;
        let message = "Test message";
        let mut cli_info = cli::CliInfo::default();

        cli_info.actions = "test".to_owned();
        cli_info.initiator = cli::Initiator::Cli;
        let distro = distro::Distro {
            architecture: distro::Architecture::X86_64,
            ..distro::Distro::default()
        };

        let envelope = create_trace_envelope(severity_level, message, &cli_info, &distro);
        assert_eq!(envelope.name, "Microsoft.ApplicationInsights.Message");
        send_envelope(&envelope)?;
        Ok(())
    }

    fn test_create_exception_envelope() -> anyhow::Result<()> {
        let severity_level = SeverityLevel::Error;
        let type_name = "TestException";
        let message = "Test exception message";
        let stack = "Test stack trace";
        let mut cli_info = cli::CliInfo::default();

        cli_info.actions = "test".to_owned();
        cli_info.initiator = cli::Initiator::Cli;
        let distro = distro::Distro {
            architecture: distro::Architecture::X86_64,
            ..distro::Distro::default()
        };

        let envelope = create_exception_envelope(
            severity_level,
            type_name,
            message,
            stack,
            &cli_info,
            &distro,
        );
        assert_eq!(envelope.name, "Microsoft.ApplicationInsights.Exception");
        send_envelope(&envelope)?;
        Ok(())
    }
    #[test]
    fn run_tests() {
        env_logger::Builder::from_env(Env::default().default_filter_or("debug")).init();
        test_create_trace_envelope().unwrap();
        test_create_exception_envelope().unwrap();
    }
}
