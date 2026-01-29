// SPDX-FileCopyrightText: 2026 Nikolay Govorov <me@govorov.online>
// SPDX-License-Identifier: AGPL-3.0-or-later

use opentelemetry::KeyValue;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{
    LogExporter, Protocol, SpanExporter, WithExportConfig, WithHttpConfig, WithTonicConfig,
};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::SdkLoggerProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use tonic::metadata::{MetadataKey, MetadataMap, MetadataValue};
use tonic::transport::{Certificate, ClientTlsConfig, Identity};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::config::{LogLevel, OtelcolConfig, StdoutFormat, TelemetryConfig};

macro_rules! build_exporter {
    ($Exporter:ident, $cfg:expr, $url:expr, $tls:expr, $path:expr) => {
        if $cfg.endpoint.starts_with("http://") {
            // with_endpoint() requires full path for HTTP protocol
            $Exporter::builder()
                .with_http()
                .with_protocol(Protocol::HttpBinary)
                .with_endpoint(format!("{}{}", $url, $path))
                .with_timeout($cfg.timeout)
                .with_headers($cfg.headers.clone())
                .build()
                .expect(concat!("failed to build ", stringify!($Exporter)))
        } else if $cfg.endpoint.starts_with("grpc://") {
            $Exporter::builder()
                .with_tonic()
                .with_endpoint($url)
                .with_timeout($cfg.timeout)
                .with_tls_config($tls)
                .with_metadata(build_metadata($cfg))
                .build()
                .expect(concat!("failed to build ", stringify!($Exporter)))
        } else {
            panic!(
                "invalid OTLP endpoint: {}. Expected http:// or grpc://",
                $cfg.endpoint
            )
        }
    };
}

fn build_metadata(cfg: &OtelcolConfig) -> MetadataMap {
    let mut metadata = MetadataMap::new();
    for (key, value) in &cfg.headers {
        if let (Ok(k), Ok(v)) = (
            key.parse::<MetadataKey<_>>(),
            value.parse::<MetadataValue<_>>(),
        ) {
            metadata.insert(k, v);
        }
    }
    metadata
}

pub struct TelemetryService {
    logger_provider: Option<SdkLoggerProvider>,
    tracer_provider: Option<SdkTracerProvider>,
}
impl Drop for TelemetryService {
    fn drop(&mut self) {
        self.shutdown();
    }
}
impl TelemetryService {
    pub fn init(config: &TelemetryConfig, service_name: &str, service_version: &str) -> Self {
        let env_filter = match std::env::var_os("ZORIAN_LOG") {
            Some(val) => tracing_subscriber::EnvFilter::try_new(val.to_string_lossy())
                .expect("Invalid ZORIAN_LOG"),
            None => tracing_subscriber::EnvFilter::new(Self::log_level_to_filter(
                config.stdout.log_level,
            )),
        };

        let mut tracer_provider = None;
        let mut logger_provider = None;

        let stdout = config
            .stdout
            .enabled
            .then(|| match config.stdout.log_format {
                StdoutFormat::Json => tracing_subscriber::fmt::layer().json().boxed(),
                StdoutFormat::Pretty => tracing_subscriber::fmt::layer().pretty().boxed(),
            });

        let (otel_logs, otel_traces) = match &config.otelcol {
            Some(cfg) if cfg.enabled => {
                let tls = Self::build_tls(cfg);
                let url = Self::parse_endpoint(&cfg.endpoint);
                let resource = Resource::builder()
                    .with_service_name(service_name.to_string())
                    .with_attribute(KeyValue::new(
                        opentelemetry_semantic_conventions::attribute::SERVICE_VERSION,
                        service_version.to_string(),
                    ))
                    .build();

                let logs = cfg.logs.then(|| {
                    let provider = SdkLoggerProvider::builder()
                        .with_resource(resource.clone())
                        .with_batch_exporter(build_exporter!(
                            LogExporter,
                            cfg,
                            url.clone(),
                            tls.clone(),
                            "/v1/logs"
                        ))
                        .build();
                    let layer =
                        opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge::new(
                            &provider,
                        )
                        .with_filter(Self::to_level_filter(cfg.log_level));
                    logger_provider = Some(provider);
                    layer
                });

                let traces = cfg.traces.then(|| {
                    let provider = SdkTracerProvider::builder()
                        .with_resource(resource.clone())
                        .with_batch_exporter(build_exporter!(
                            SpanExporter,
                            cfg,
                            url.clone(),
                            tls.clone(),
                            "/v1/traces"
                        ))
                        .build();
                    let layer = tracing_opentelemetry::layer()
                        .with_tracer(provider.tracer(service_name.to_string()));
                    tracer_provider = Some(provider);
                    layer
                });

                (logs, traces)
            }
            _ => (None, None),
        };

        tracing_subscriber::registry()
            .with(stdout)
            .with(otel_logs)
            .with(otel_traces)
            .with(env_filter)
            .init();

        Self {
            tracer_provider,
            logger_provider,
        }
    }

    fn parse_endpoint(endpoint: &str) -> String {
        if endpoint.starts_with("http://") {
            endpoint.to_string()
        } else if let Some(rest) = endpoint.strip_prefix("grpc://") {
            format!("https://{rest}")
        } else {
            panic!("invalid OTLP endpoint: {endpoint}. Expected http:// or grpc://")
        }
    }

    fn build_tls(cfg: &OtelcolConfig) -> ClientTlsConfig {
        let mut tls = ClientTlsConfig::new().with_native_roots();
        if let Some(path) = &cfg.tls_ca {
            tls = tls.ca_certificate(Certificate::from_pem(
                std::fs::read_to_string(path).expect("failed to read CA"),
            ));
        }
        if let (Some(crt), Some(key)) = (&cfg.tls_crt, &cfg.tls_key) {
            tls = tls.identity(Identity::from_pem(
                std::fs::read_to_string(crt).expect("failed to read cert"),
                std::fs::read_to_string(key).expect("failed to read key"),
            ));
        }
        tls
    }

    fn to_level_filter(level: LogLevel) -> tracing_subscriber::filter::LevelFilter {
        match level {
            LogLevel::Trace => tracing_subscriber::filter::LevelFilter::TRACE,
            LogLevel::Debug => tracing_subscriber::filter::LevelFilter::DEBUG,
            LogLevel::Info => tracing_subscriber::filter::LevelFilter::INFO,
            LogLevel::Warning => tracing_subscriber::filter::LevelFilter::WARN,
            LogLevel::Error => tracing_subscriber::filter::LevelFilter::ERROR,
        }
    }

    fn log_level_to_filter(level: LogLevel) -> &'static str {
        match level {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warning => "warn",
            LogLevel::Error => "error",
        }
    }

    pub fn shutdown(&mut self) {
        if let Some(p) = self.tracer_provider.take()
            && let Err(e) = p.shutdown()
        {
            tracing::error!("failed to shutdown tracer provider: {e}");
        }
        if let Some(p) = self.logger_provider.take()
            && let Err(e) = p.shutdown()
        {
            tracing::error!("failed to shutdown logger provider: {e}");
        }
    }
}
