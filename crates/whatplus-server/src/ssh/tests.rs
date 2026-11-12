//! 端到端：真的啟動 SSH 服務，用 whatplus-ssh 用戶端連線。

use std::sync::Arc;

use what_core::{Source, TokenCounts, UsageRecord};
use whatplus_proto::{AgentKeyInput, Request, Response, Role};
use whatplus_ssh::client::{ConnectError, ConnectOptions, Connection};
use whatplus_ssh::keys;

use super::serve_on;
use crate::app::App;
use crate::auth::{AuthHolder, AuthSnapshot};
use crate::config::Settings;
use crate::db::{NewKey, Store};
use crate::discord::Discord;

struct Fixture {
    port: u16,
    host_fp: String,
    store: Arc<Store>,
}

fn new_key(name: &str) -> (Arc<whatplus_ssh::PrivateKey>, String) {
    let (private, public) = keys::generate_ed25519(name).unwrap();
    let key = whatplus_ssh::PrivateKey::from_openssh(private.as_bytes()).unwrap();
    (Arc::new(key), public)
}

fn register(store: &Store, name: &str, role: Role, public: &str) -> i64 {
    let key = keys::parse_ed25519_public_key(public).unwrap();
    store
        .add_key(NewKey {
            name,
            role,
            public_key: public,
            fingerprint: &keys::fingerprint(&key),
            allowed_cidrs: &[],
        })
        .unwrap()
}

async fn start(store: Arc<Store>) -> Fixture {
    let (private, _) = keys::generate_ed25519("host").unwrap();
    let host_key = whatplus_ssh::PrivateKey::from_openssh(private.as_bytes()).unwrap();
    let host_fp = keys::fingerprint(host_key.public_key());
    let settings = Settings::parse("timezone=Asia/Taipei").unwrap();
    let app = Arc::new(App {
        store: store.clone(),
        auth: Arc::new(AuthHolder::new(AuthSnapshot::load(&store).unwrap())),
        discord: Discord::new(Err(anyhow::anyhow!("no token in tests")), store.clone()),
        settings,
        host_fingerprint: host_fp.clone(),
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(serve_on(listener, app, host_key));
    Fixture { port, host_fp, store }
}

fn opts(f: &Fixture, key: &Arc<whatplus_ssh::PrivateKey>, known: Option<&str>) -> ConnectOptions {
    ConnectOptions {
        host: "127.0.0.1".into(),
        port: f.port,
        key: key.clone(),
        known_fingerprint: known.map(str::to_string),
        client_name: "test".into(),
    }
}

fn record() -> UsageRecord {
    UsageRecord {
        source: Source::ClaudeCode,
        timestamp: jiff::Timestamp::now(),
        model: "claude-test".into(),
        project: "proj".into(),
        session_id: "s".into(),
        tokens: TokenCounts { input: 100, output: 50, ..Default::default() },
        cost_usd: 0.25,
        pricing_missing: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn tofu_roles_and_upload() {
    let store = Arc::new(Store::open_in_memory().unwrap());
    store.add_allowed_network("127.0.0.0/8").unwrap();
    let (admin_key, admin_pub) = new_key("admin");
    let (agent_key, agent_pub) = new_key("agent");
    let (stranger_key, _) = new_key("stranger");
    register(&store, "me", Role::Admin, &admin_pub);
    let f = start(store).await;

    // 第一次連線：回報伺服器指紋，不登入。
    match Connection::connect(opts(&f, &admin_key, None)).await {
        Err(ConnectError::UnknownHost { fingerprint }) => assert_eq!(fingerprint, f.host_fp),
        other => panic!("expected UnknownHost, got {:?}", other.err()),
    }
    // 指紋不符。
    let wrong = "SHA256:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
    assert!(matches!(
        Connection::connect(opts(&f, &admin_key, Some(wrong))).await,
        Err(ConnectError::HostKeyChanged { .. })
    ));
    // 未註冊的金鑰。
    assert!(matches!(
        Connection::connect(opts(&f, &stranger_key, Some(&f.host_fp))).await,
        Err(ConnectError::AuthRejected)
    ));

    // admin 新增 agent。
    let mut admin = Connection::connect(opts(&f, &admin_key, Some(&f.host_fp))).await.unwrap();
    assert_eq!(admin.role, Role::Admin);
    let added = admin
        .request(Request::AddAgentKey {
            key: AgentKeyInput { name: "desktop".into(), public_key: agent_pub, allowed_cidrs: vec![] },
        })
        .await
        .unwrap();
    assert!(matches!(added, Response::Saved { .. }));
    // admin 不能上傳。
    assert!(admin.request(Request::Upload { records: vec![] }).await.is_err());

    // agent 上傳，重送不重複計算；不能讀設定。
    let mut agent = Connection::connect(opts(&f, &agent_key, Some(&f.host_fp))).await.unwrap();
    assert_eq!(agent.role, Role::Agent);
    let r = record();
    agent.request(Request::Upload { records: vec![r.clone()] }).await.unwrap();
    agent.request(Request::Upload { records: vec![r] }).await.unwrap();
    match agent.request(Request::UploadState).await.unwrap() {
        Response::UploadState { records, latest } => {
            assert_eq!(records, 1);
            assert!(latest.is_some());
        }
        other => panic!("{other:?}"),
    }
    assert!(agent.request(Request::ListKeys).await.is_err());
    agent.close().await;

    // admin 看得到 agent，不能停用自己（admin 公鑰）。
    let Response::Keys { keys } = admin.request(Request::ListKeys).await.unwrap() else { panic!() };
    let me = keys.iter().find(|k| k.role == Role::Admin).unwrap();
    assert!(admin.request(Request::SetAgentKeyEnabled { id: me.id, enabled: false }).await.is_err());
    let desktop = keys.iter().find(|k| k.name == "desktop").unwrap();
    assert_eq!(desktop.record_count, 1);

    // 預覽報告包含機器名稱。
    let Response::Preview { text } = admin
        .request(Request::PreviewReport {
            range: whatplus_proto::ManualRange::Today,
            content: Default::default(),
            language: whatplus_proto::Language::En,
        })
        .await
        .unwrap()
    else {
        panic!()
    };
    assert!(text.contains("desktop"), "{text}");

    // 停用後 agent 無法登入。
    admin.request(Request::SetAgentKeyEnabled { id: desktop.id, enabled: false }).await.unwrap();
    assert!(matches!(
        Connection::connect(opts(&f, &agent_key, Some(&f.host_fp))).await,
        Err(ConnectError::AuthRejected)
    ));
    admin.close().await;
    drop(f.store);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_outside_allowlist_and_without_admin() {
    // 白名單是空的：所有連線在握手前就被關閉。
    let store = Arc::new(Store::open_in_memory().unwrap());
    let (admin_key, admin_pub) = new_key("admin");
    register(&store, "me", Role::Admin, &admin_pub);
    let f = start(store).await;
    assert!(matches!(
        Connection::connect(opts(&f, &admin_key, Some(&f.host_fp))).await,
        Err(ConnectError::Network(_))
    ));

    // 沒有 admin 公鑰：即使 IP 在白名單內也拒絕。
    let store = Arc::new(Store::open_in_memory().unwrap());
    store.add_allowed_network("127.0.0.0/8").unwrap();
    let f = start(store).await;
    assert!(matches!(
        Connection::connect(opts(&f, &admin_key, None)).await,
        Err(ConnectError::Network(_))
    ));
}
