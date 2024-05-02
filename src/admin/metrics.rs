use crate::Rpc;
use http_body_util::Full;
use hyper::{
    body::Bytes,
    server::conn::http1,
    service::service_fn,
    Request,
};
use prometheus::Error;
use prometheus_metric_storage::{
    MetricStorage,
    StorageRegistry,
};
use serde_json::{
    json,
    Value,
    Value::Null,
};
use std::convert::Infallible;
use std::{
    collections::hash_map::HashMap,
    sync::{
        Arc,
        RwLock,
    },
    time::Duration,
};

use crate::{
    admin::error::AdminError,
    Settings,
};

use tokio::sync::{
    mpsc::{
        unbounded_channel,
        UnboundedReceiver,
        UnboundedSender,
    },
    oneshot,
};
use tokio::time::interval;

//TODO: have fns accept a refernce to StorageRegistry
// refer to https://docs.rs/prometheus-metric-storage/latest/prometheus_metric_storage/#metric-storage-registry
type CounterMap = HashMap<(String, u64), RpcMetrics>;
pub type MetricSender = UnboundedSender<RpcMetrics>;
pub type MetricReceiver = UnboundedReceiver<RpcMetrics>;
pub type MetricUpdateSender = tokio::sync::mpsc::Sender<MetricsUpdate>;
pub type MetricUpdateReciever = tokio::sync::mpsc::Receiver<MetricsUpdate>;

const VERSION_LABEL: [(&str, &str); 1] = [("version", env!("CARGO_PKG_VERSION"))];
// #[derive(Debug)]
// pub enum MetricsError {
//     WSError(String),
//     RpcError(String),
// }

#[derive(MetricStorage, Clone, Debug)]
#[metric(subsystem = "rpc")]
pub struct RpcMetrics {
    #[metric(labels("url", "method", "status"), help = "Total number of requests")]
    pub requests: prometheus::IntCounterVec,
    #[metric(labels("url", "method"), help = "latency of request")]
    pub duration: prometheus::HistogramVec,
}
impl RpcMetrics {
    pub fn init(registry: &StorageRegistry) -> Result<&Self, prometheus::Error> {
        RpcMetrics::instance(registry)
    }
    pub fn requests_complete(&self, url: &str, method: &str, status: &str, dt: Duration) {
        self.requests
            .with_label_values(&[url, method, &status.to_string()])
            .inc();
        self.duration
            .with_label_values(&[url, method])
            .observe(dt.as_millis() as f64)
    }
}

pub async fn execute_metric_method(
    tx: Value,
    rpc_list: &Arc<RwLock<Vec<Rpc>>>,
    config: Arc<RwLock<Settings>>,
    registry_state: Arc<RwLock<StorageRegistry>>,
    cache: sled::Db,
) -> Result<Value, Error> {
    unimplemented!()
}

fn metrics_config(config: Arc<RwLock<Settings>>) -> Result<Value, Error> {
    let guard = config.read().unwrap();
    let rx = json!({
        "id": Null,
        "jsonrpc": "2.0",
        "result": {
            "metrics": {
                "enabled": guard.metrics.enabled,
                "address": guard.metrics.address,
                "interval": guard.metrics.count_update_interval,
            },
        },

    });
    Ok(rx)
}
// #[cfg(feature = "prometheusd")]
// #[derive(Debug)]
// pub enum MetricsCommand<'a> {
//     Flush(),
//     Channel(&'a MetricSender, &'a MetricReceiver),
//     PushLatency(&'a RpcMetrics, &'a str, &'a str, f64),
//     PushRequest(&'a RpcMetrics, &'a str, &'a str, &'a u16, Duration),
//     PushError(&'a RpcMetrics, &'a str, &'a str, &'a u16, Duration),
//     Push(&'a RpcMetrics),
//     Pull(&'a RpcMetrics),
// }

// #[cfg(feature = "prometheusd")]
// #[derive(Debug)]
// pub enum MetricChannelCommand {
//     AdminMsg(oneshot::Sender<RpcMetrics>),
//     StatsMsg(oneshot::Sender<RpcMetrics>),
// }

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MetricsUpdate {
    Http,
    Websocket,
    Database,
}

#[cfg(not(feature = "prometheusd"))]
pub async fn metrics_update_sink(mut metrics_rx: MetricReceiver) {
    loop {
        while metrics_rx.recv().await.is_some() {
            continue;
        }
    }
}

#[cfg(feature = "prometheusd")]
pub async fn listen_for_metrics_requests(
    config: Arc<RwLock<Settings>>,
    metrics_rx: MetricUpdateReciever,
    registry_status: Arc<RwLock<StorageRegistry>>,
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    dt: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::config::{
        cache_setup::setup_data,
        cli_args::create_match,
    };
    let (address, interval) = {
        let config_guard = config.read().unwrap();
        (
            config_guard.metrics.address,
            config_guard.metrics.count_update_interval,
        )
    };

    let (metrics_request_tx, metrics_request_rx) = metrics_update_channel().await;
    let (metrics_tx, metrics_rx) = metrics_channel().await;
    let metrics_request_tx_rwlock = Arc::new(RwLock::new(metrics_request_tx.clone()));
    let metrics_tx_rwlock = Arc::new(RwLock::new(metrics_tx.clone()));
    tokio::spawn(metrics_monitor(
        metrics_request_rx,
        registry_status.clone(),
        rpc_list.clone(),
        dt,
    ));
    metrics_server(
        metrics_tx_rwlock,
        registry_status,
        metrics_tx,
        address,
        config,
    )
    .await
}
#[cfg(feature = "prometheusd")]
pub async fn metrics_listener(
    mut metrics_rx: MetricUpdateReciever,
    metrics_status: Arc<RwLock<RpcMetrics>>,
    rpc_list_str: &str,
    _method: &str,
    dt: Duration,
) {
    while let Some(update) = metrics_rx.recv().await {
        match update {
            MetricsUpdate::Http => {
                let mut metrics_guard = metrics_status.write().unwrap();
                metrics_guard.requests_complete(rpc_list_str, &"http", &"200", dt);
            }
            MetricsUpdate::Websocket => {
                let mut metrics_guard = metrics_status.write().unwrap();
                metrics_guard.requests_complete(rpc_list_str, &"WebSocket", &"200", dt);
            }
            MetricsUpdate::Database => {
                let mut metrics_guard = metrics_status.write().unwrap();
                metrics_guard.requests_complete(rpc_list_str, &"Database", &"200", dt);
            }
        }
    }
}
#[cfg(feature = "prometheusd")]
pub async fn metrics_processor(
    mut metrics_rx: MetricReceiver,
    registry_state: Arc<RwLock<StorageRegistry>>,
) {
    use crate::log_info;
    let _metrics = RpcMetrics::init(&registry_state.read().unwrap()).unwrap();
    loop {
        while let Some(incoming) = metrics_rx.recv().await {
            incoming.requests_complete("test", "test", &"200", Duration::from_millis(100));
            let test_report = metrics_encoder(registry_state.clone()).await;
            log_info!("prometheus metrics: {:?}", test_report);
            let _registry = registry_state.read().unwrap();
        }
    }
}
/// Accepts metrics request, encodes and prints
#[cfg(feature = "prometheusd")]
pub async fn execute_write_metrics(
    tx: Value,
    metrics_tx: Arc<RwLock<MetricSender>>,
    registry_state: Arc<RwLock<StorageRegistry>>,
    metrics: Arc<RwLock<RpcMetrics>>,
    dt: Duration,
) -> Result<Value, AdminError> {
    use crate::log_info;
    let method = tx["method"].as_str().unwrap();
    match method {
        Some("blutgang_set_ttl") => {
            if write_protection_enabled {
                Err(AdminError::WriteProtectionEnabled)
            } else {
                //write + collect metrics after matching for method
                let dt = std::time::Instant::now();
                admin_blutgang_set_ttl(config, tx["params"].as_array());
                metrics
                    .read()
                    .unwrap()
                    .requests_complete(method, url, &"200", dt.elapsed());
            }
        }
    }

    let metrics_report = metrics_encoder(registry_clone).await;
    let rx = json!({
        "jsonrpc": "2.0",
        "metrics_result": metrics_report,
    });
    log_info!("metrics response: {:?}", rx);
    Ok(rx)
}

/// Accepts metrics request, encodes and prints
#[cfg(feature = "prometheusd")]
pub async fn write_metrics_response(
    tx: Request<hyper::body::Incoming>,
    metrics_tx: Arc<RwLock<MetricSender>>,
    registry_state: Arc<RwLock<StorageRegistry>>,
) -> Result<hyper::Response<Full<Bytes>>, Infallible> {
    use crate::{
        balancer::format::incoming_to_value,
        log_info,
    };
    use serde_json::{
        json,
        Value,
        Value::Null,
    };

    let metrics_report = metrics_encoder(registry_state).await;
    let response = Ok(hyper::Response::builder()
        .status(200)
        .body(Full::from(Bytes::from(metrics_report)))
        .unwrap());
    log_info!("metrics response: {:?}", response);
    (response)
}
#[cfg(feature = "prometheusd")]
async fn metrics_encoder(storage_registry: Arc<RwLock<StorageRegistry>>) -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buffer = vec![];
    let registry = storage_registry.read().unwrap().gather();
    encoder.encode(&registry, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}

//listens for updates to metrics and updates the storage registry based on interval
#[cfg(feature = "prometheusd")]
pub(in crate::r#admin) async fn metrics_monitor(
    metrics_rx: MetricUpdateReciever,
    storage_registry: Arc<RwLock<StorageRegistry>>,
    rpc_list: Arc<RwLock<Vec<Rpc>>>,
    dt: Duration,
) {
    let registry;
    let registry_guard = storage_registry.read().unwrap();
    registry = registry_guard;
    let metrics_status = Arc::new(RwLock::new(
        RpcMetrics::instance(&registry).unwrap().to_owned(),
    ));
    let rpc_list = rpc_list
        .read()
        .map_err(|_| AdminError::Inaccessible)
        .unwrap();
    let mut rpc_list_str = String::new();
    rpc_list_str.push('[');
    for rpc in rpc_list.iter() {
        rpc_list_str.push_str(&format!("{{\"name\": \"{}\", }}", rpc.name,));
    }
    rpc_list_str.push(']');

    let metrics_stat_listener = metrics_status.clone();
    tokio::spawn(metrics_listener(
        metrics_rx,
        metrics_stat_listener,
        "www.example.com",
        &"metrics",
        dt,
    ));
}
pub async fn metrics_channel() -> (MetricSender, MetricReceiver) {
    let (tx, rx) = unbounded_channel();
    (tx, rx)
}

pub async fn metrics_update_channel() -> (MetricUpdateSender, MetricUpdateReciever) {
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    (tx, rx)
}

#[cfg(feature = "prometheusd")]
pub async fn close(rx: &mut MetricReceiver) {
    rx.close();
}
#[cfg(feature = "prometheusd")]
pub async fn metrics_server(
    metrics_tx: Arc<RwLock<MetricSender>>,
    registry_state: Arc<RwLock<StorageRegistry>>,
    metrics_request_tx: MetricSender,
    address: std::net::SocketAddr,
    config: Arc<RwLock<Settings>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crate::accept_prometheusd;
    use crate::log_info;
    use hyper_util_blutgang::rt::TokioIo;
    use tokio::{
        net::TcpListener,
        sync::mpsc,
    };
    let update_interval = {
        let config_guard = config.read().unwrap();
        config_guard.metrics.count_update_interval
    };
    let listener = TcpListener::bind(address).await?;
    log_info!("Bound metrics to : {}", address);
    let mut interval = tokio::time::interval(Duration::from_secs(update_interval));

    loop {
        let (stream, socketaddr) = listener.accept().await?;
        log_info!("Metrics connection from: {}", socketaddr);
        let io = TokioIo::new(stream);
        let config_clone = Arc::clone(&config);
        let registry_clone = Arc::clone(&registry_state);
        let tx_clone = Arc::clone(&metrics_tx);
        tokio::task::spawn(async move {
            accept_prometheusd!(io, &tx_clone, &registry_clone, metrics_request_tx,);
        });
    }
}

// #[cfg(feature = "prometheusd")]
// #[macro_export]
// macro_rules! set_metric {
//     ($data: expr, $version:expr , $type_label:expr) => {

//     };
// }

#[cfg(feature = "prometheusd")]
#[macro_export]
macro_rules! accept_prometheusd {
    (
     $io:expr,
     $http_tx:expr,
     $registry_state:expr,
     $metrics_request_tx:expr,
    ) => {
        use crate::admin::metrics::write_metrics_response;
        if let Err(err) = http1::Builder::new()
            .serve_connection(
                $io,
                service_fn(|req| {
                    let response = write_metrics_response(
                        req,
                        Arc::clone($http_tx),
                        Arc::clone($registry_state),
                    );
                    response
                }),
            )
            .await
        {
            println!("Error serving prometheus metrics: {:?}", err);
        }
    };
}

pub mod test_mocks {
    use super::*;
    use crate::admin::metrics::RpcMetrics;
    use crate::Rpc;
    use rand::Rng;
    //borrowed from admin
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        )]))
    }

    #[derive(Debug)]
    struct MockRpcs {
        pub rpc_list: Arc<RwLock<Vec<Rpc>>>,
    }

    // #[derive(Debug, MetricStorage)]
    // #[metric(subsystem = "test")]
    // pub struct MockMetrics {
    //     #[metric(labels("path", "method", "status"), help = "Total number of requests")]
    //     pub requests: prometheus::IntCounterVec,
    //     #[metric(labels("path", "method"), help = "latency of request")]
    //     #[metric(buckets(0.1, 0.2, 0.5, 1, 2, 4, 8))]
    //     pub request_duration: prometheus::HistogramVec,
    //     }
    // }

    #[derive(Debug)]
    pub struct MockRpcMetrics {
        pub inner: RpcMetrics,
    }

    impl MockRpcMetrics {
        fn gen_metrics(&self, mut rng: rand::rngs::StdRng) {
            for _ in 0..5 {
                let rand_status = rng.gen_range(0..=2);
                let rand_duration = rng.gen_range(1..=100);
                match rand_status {
                    0 => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"200",
                            Duration::from_millis(rand_duration),
                        )
                    }
                    1 => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"202",
                            Duration::from_millis(rand_duration),
                        )
                    }
                    2 => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"503",
                            Duration::from_millis(rand_duration),
                        )
                    }
                    _ => {
                        self.inner.requests_complete(
                            "test",
                            "test",
                            &"500",
                            Duration::from_millis(rand_duration),
                        )
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[cfg(feature = "prometheusd")]
mod tests {
    use self::test_mocks::MockRpcMetrics;
    use crate::admin::metrics::{
        listen_for_metrics_requests,
        metrics_channel,
        metrics_monitor,
    };

    use super::*;
    use crate::config::{
        cache_setup::setup_data,
        cli_args::create_match,
        types::Settings,
    };
    use crate::log_info;
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        )]))
    }
    fn create_test_settings_config() -> Arc<RwLock<Settings>> {
        let mut config = Settings::default();
        config.do_clear = true;
        config.admin.key = DecodingKey::from_secret(b"some-key");
        Arc::new(RwLock::new(config))
    }

    // Helper function to create a test cache
    fn create_test_cache() -> Db {
        let db = sled::Config::new().temporary(true);
        let db = db.open().unwrap();

        (db)
    }

    async fn mock_setup() {
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let config = Arc::new(RwLock::new(Settings::new(create_match()).await));
        let config_metrics = Arc::clone(&config);
        let metrics_addr = config_metrics.read().unwrap().metrics.address.clone();
        let update_interval = config_metrics
            .read()
            .unwrap()
            .metrics
            .count_update_interval
            .clone();
        log_info!(
            "mock metrics settings: address: {}, update interval: {}",
            metrics_addr,
            update_interval
        );

        let storage_registry = prometheus_metric_storage::StorageRegistry::default();
        //TODO: why do i gotta clone this?
        let mock_metrics = MockRpcMetrics {
            inner: RpcMetrics::init(&storage_registry).unwrap().clone(),
        };
        let metrics_tx_rwlock = Arc::new(RwLock::new(metrics_tx));
        let registry_rwlock = Arc::new(RwLock::new(storage_registry));
        let registry_clone = Arc::clone(&registry_rwlock);
        tokio::task::spawn(async move {
            log_info!("Prometheus enabled, accepting metrics at prometheus port");
            let _ = listen_for_metrics_requests(config_metrics, metrics_rx, registry_rwlock).await;
        });
    }
    async fn assert_metrics() {}
    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    //RUST_LOG=info cargo test --config example_config.toml -F prometheusd
    async fn test_prometheus_listener() {
        use crate::config::cli_args::create_match;
        let mut config = Settings::default();
        Arc::new(RwLock::new(config));

        let storage = StorageRegistry::default();
        let storage_arc = Arc::new(RwLock::new(storage));
        let config = Arc::new(RwLock::new(Settings::new(create_match()).await));
        let config_metrics = Arc::clone(&config);

        let dt = std::time::Instant::now();
        log_info!(
            "Initial metrics state: {:?}",
            storage_arc.read().unwrap().gather()
        );
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let storage_clone = Arc::clone(&storage_arc);
        let storage_guard = storage_arc.read().unwrap();
        let mut rpc_metrics = RpcMetrics::init(&storage_guard).unwrap();
        rpc_metrics.requests_complete("test", "test", &"200", dt.elapsed());
        listen_for_metrics_requests(config_metrics, metrics_rx, storage_clone);
        let test_report = metrics_encoder(storage_arc.clone()).await;
        log_info!("metrics state: {:?}", test_report);
    }

    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    //RUST_LOG=info cargo test --features prometheusd -- test_prometheus_server --nocapture
    async fn test_prometheus_server() {
        use crate::config::types::Settings;
        use crate::create_match;
        use hyper_util_blutgang::rt::TokioIo;
        use tokio::net::TcpListener;

        let storage = StorageRegistry::default();
        let storage_arc = Arc::new(RwLock::new(storage));
        let dt = std::time::Instant::now();
        log_info!(
            "Initial metrics state: {:?}",
            storage_arc.read().unwrap().gather()
        );
        let dt = std::time::Instant::now();
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let storage_clone = Arc::clone(&storage_arc);
        let storage_guard = storage_arc.read().unwrap();
        let mut rpc_metrics = RpcMetrics::init(&storage_guard).unwrap();
        for _ in 0..3 {
            rpc_metrics.requests_complete("test", "test", &"200", dt.elapsed());
            // listen_for_metrics_requests(metrics_rx, storage_clone.clone());
            let test_report = metrics_encoder(storage_arc.clone()).await;
            log_info!("metrics state: {:?}", test_report);
        }
    }
    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    async fn test_execute_methods_metrics_rpc_config() {
        use crate::admin::metrics::metrics_channel;
        use crate::admin::metrics::write_metrics_val;
        use crate::log_info;
        let cache = create_test_cache();
        let config = create_test_settings_config();
        let guard = config.read().unwrap();
        let (metrics_tx, metrics_rx) = metrics_channel().await;
        let metrics_tx_rwlock = Arc::new(RwLock::new(metrics_tx));
        let storage = StorageRegistry::default();
        let storage_rwlock = Arc::new(RwLock::new(storage));
        let dt = Instant::now();
        let rx = json!({
            "id": Null,
            "jsonrpc": "2.0",
            "method": "blutgang_config",
            "path": "/rpc",
            "status": "200",
            "result": {
                "address": guard.address,
                "do_clear": guard.do_clear,
                "health_check": guard.health_check,
                "admin": {
                    "enabled": guard.admin.enabled,
                    "readonly": guard.admin.readonly,
                },
                "ttl": guard.ttl,
                "health_check_ttl": guard.health_check_ttl,
            },
        });
        let metrics = write_metrics_val(rx, metrics_tx_rwlock, storage_rwlock, dt.elapsed()).await;
    }

    #[cfg(feature = "prometheusd")]
    #[tokio::test]
    //RUST_LOG=info cargo test --features prometheusd -- test_metrics_e2e --nocapture
    async fn test_metrics_e2e() {}
}
