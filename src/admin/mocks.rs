use crate::{
    admin::error::AdminError,
    Rpc,
    Settings,
    NamedBlocknumbers,
    
    log_err,
    log_info,
    websocket::{
        error::WsError,
        types::{
            IncomingResponse,
            SubscriptionData,
            WsChannelErr,
            WsconnMessage,
            RequestResult,
        },
    }
};
use tokio::sync::broadcast;
use tokio::sync::mpsc::{
    self,
    UnboundedSender,
    UnboundedReceiver,
};
use std::{
    sync::{
        Arc,
        RwLock,
    },
    time::Instant,
};
use serde_json::{
    json,
    value,
    
};


use jsonwebtoken::DecodingKey;

fn dummy_named_blocknumbers() -> Arc<RwLock<NamedBlocknumbers>> {
        Arc::new(RwLock::new(NamedBlocknumbers {
            latest: 10,
            earliest: 2,
            safe: 3,
            finalized: 4,
            pending: 5,
        }))
    }

    fn mock_rpc(url: &str) -> Rpc {
        Rpc::new(
            format!("http://{}", url),
            Some(format!("ws://{}", url)),
            10000,
            1,
            10.0,
        )
    }


    // Helper function to create a test RPC list
    fn create_test_rpc_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://example.com".to_string(),
            None,
            5,
            1000,
            0.5,
        )]))
    }

    // Helper function to create a test poverty list
    fn create_test_poverty_list() -> Arc<RwLock<Vec<Rpc>>> {
        Arc::new(RwLock::new(vec![Rpc::new(
            "http://poverty.com".to_string(),
            None,
            2,
            1000,
            0.1,
        )]))
    }
    fn setup_ws_conn_manager_test() -> (
        Arc<RwLock<Vec<Rpc>>>,
        mpsc::UnboundedSender<WsconnMessage>,
        mpsc::UnboundedReceiver<WsconnMessage>,
        broadcast::Sender<IncomingResponse>,
        mpsc::UnboundedSender<WsChannelErr>,
    ) {
        let rpc_list = Arc::new(RwLock::new(vec![
            mock_rpc("node1.example.com"),
            mock_rpc("node2.example.com"),
        ]));
        let (incoming_tx, incoming_rx) = mpsc::unbounded_channel();
        let (broadcast_tx, _) = broadcast::channel(10);
        let (ws_error_tx, _) = mpsc::unbounded_channel();

        (
            rpc_list,
            incoming_tx,
            incoming_rx,
            broadcast_tx,
            ws_error_tx,
        )
    }
    fn setup_user_and_subscription_data() -> (
        SubscriptionData,
        u32,
        mpsc::UnboundedReceiver<RequestResult>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        let user_data = tx;
        let user_id = 100;
        let subscription_data = SubscriptionData::new();
        subscription_data.add_user(user_id, user_data);
        (subscription_data, user_id, rx)
    }
    // Helper function to create a test Settings config
    fn create_test_settings_config() -> Arc<RwLock<Settings>> {
        let mut config = Settings::default();
        config.do_clear = true;
        config.admin.key = DecodingKey::from_secret(b"some-key");
        Arc::new(RwLock::new(config))
    }

    // Helper function to create a test cache
    fn create_test_cache() -> sled::Db {
        let db = sled::Config::new().temporary(true);

        db.open().unwrap()
    }


