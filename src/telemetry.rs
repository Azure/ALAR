use std::collections::HashMap;
use crate::cli;
use crate::distro;
use crate::global;
use crate::helper;
use chrono::Utc;
use log::debug;
use reqwest::blocking::Client;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Serialize;
use std::env;
use std::time::Duration;

#[derive(Serialize, Debug)]
pub enum SeverityLevel {
    Verbose,
    Information,
    Warning,
    Error,
    Critical,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct ExceptionBaseData {
    ver: u8,
    exceptions: Vec<Exceptions>,
    severityLevel: SeverityLevel,
    properties: HashMap<String, String>,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct TraceBaseData {
    ver: u8,
    message: String,
    severityLevel: SeverityLevel,
    properties: HashMap<String, String>,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct Exceptions {
    typeName: String,
    message: String,
    stack: String,
    hasFullStack: bool,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct TraceBase {
    baseType: String,
    baseData: TraceBaseData,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct ExceptionBase {
    baseType: String,
    baseData: ExceptionBaseData,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct TraceEnvelope {
    name: String,
    time: String,
    iKey: String,
    tags: HashMap<String, String>,
    data: TraceBase,
}

#[derive(Serialize, Debug)]
#[allow(non_snake_case)]
pub struct ExceptionEnvelope {
    name: String,
    time: String,
    iKey: String,
    tags: HashMap<String, String>,
    data: ExceptionBase,
}

#[derive(Debug, Clone)]
#[allow(non_snake_case)]
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

pub(crate) fn create_exception_envelope(
    severity_level: SeverityLevel,
    type_name: &str,
    message: &str,
    stack: &str,
    cli_info: &cli::CliInfo,
    distro: &distro::Distro,
) -> ExceptionEnvelope {
    let repair_info = OsNameArchitecture::new(distro.architecture);

    // Properties for baseData
    let properties: HashMap<String, String> = HashMap::from([
        (
            "Initiator".to_owned(),
            match cli_info.initiator {
                cli::Initiator::Cli => "CLI".to_owned(),
                cli::Initiator::RecoverVm => "RecoverVm".to_owned(),
                cli::Initiator::SelfHelp => "SelfHelp".to_owned(),
            },
        ),
        (
            "Action".to_owned(),
            cli_info.actions.clone(),
        ),
        ("Architecture".to_owned(), repair_info.arch),
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
    ]);

    ExceptionEnvelope {
        name: "Microsoft.ApplicationInsights.Exception".to_owned(),
        time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        iKey: global::get_ikey(),
        tags: HashMap::from([
            ("ai.cloud.role".to_owned(), "ALAR".to_owned()),
            (
                "ai.internal.sdkVersion".to_owned(),
                clap::crate_version!().to_owned(),
            ),
            (
                "Initiator".to_owned(),
                match cli_info.initiator {
                    cli::Initiator::Cli => "CLI".to_owned(),
                    cli::Initiator::RecoverVm => "RecoverVm".to_owned(),
                    cli::Initiator::SelfHelp => "SelfHelp".to_owned(),
                },
            ),
            (
                "Action".to_owned(),
                cli_info.actions.clone(),
            ),
        ]),
        data: ExceptionBase {
            baseType: "ExceptionData".to_owned(),
            baseData: ExceptionBaseData {
                ver: 2,
                exceptions: vec![Exceptions {
                    typeName: type_name.to_owned(),
                    message: message.to_owned(),
                    stack: stack.to_owned(),
                    hasFullStack: true,
                }],
                severityLevel: severity_level,
                properties,
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

    let initiator = match cli_info.initiator {
        cli::Initiator::Cli => "CLI".to_owned(),
        cli::Initiator::RecoverVm => "RecoverVm".to_owned(),
        cli::Initiator::SelfHelp => "SelfHelp".to_owned(),
    };

    // Properties for baseData
    let properties = HashMap::from([
        ("Initiator".to_owned(), initiator),
        (
            "Action".to_owned(),
            String::from(&cli_info.actions),
        ),
        ("Architecture".to_owned(), repair_info.arch),
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
    ]);

    TraceEnvelope {
        name: "Microsoft.ApplicationInsights.Message".to_owned(),
        time: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        iKey: global::get_ikey(),
        tags: HashMap::from([
            ("ai.cloud.role".to_owned(), "ALAR".to_owned()),
            (
                "ai.internal.sdkVersion".to_owned(),
                clap::crate_version!().to_owned(),
            ),
        ]),
        data: TraceBase {
            baseType: "MessageData".to_owned(),
            baseData: TraceBaseData {
                ver: 2,
                message: message.to_owned(),
                severityLevel: severity_level,
                properties,
            },
        },
    }
}

pub(crate) fn send_envelope<T: Serialize>(envelope: &T) -> anyhow::Result<()> {
    let endpoint = global::get_endpoint();

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
