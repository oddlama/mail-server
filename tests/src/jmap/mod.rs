use std::{sync::Arc, time::Duration};

use jmap::{api::JmapSessionManager, JMAP};
use jmap_client::client::{Client, Credentials};
use jmap_proto::types::id::Id;
use tokio::sync::watch;

use crate::{add_test_certs, store::TempDir};

pub mod auth_acl;
pub mod auth_limits;
pub mod auth_oauth;
pub mod email_changes;
pub mod email_copy;
pub mod email_get;
pub mod email_parse;
pub mod email_query;
pub mod email_query_changes;
pub mod email_search_snippet;
pub mod email_set;
pub mod event_source;
pub mod mailbox;
pub mod push_subscription;
pub mod thread_get;
pub mod thread_merge;

const SERVER: &str = "
[server]
hostname = 'jmap.example.org'

[server.listener.jmap]
bind = ['127.0.0.1:8899']
url = 'https://127.0.0.1:8899'
protocol = 'jmap'
max-connections = 512

[server.socket]
reuse-addr = true

[server.tls]
enable = true
implicit = false
certificate = 'default'

[store]
db.path = '{TMP}/sqlite.db'
blob.path = '{TMP}'

[certificate.default]
cert = 'file://{CERT}'
private-key = 'file://{PK}'

[jmap.protocol]
set.max-objects = 100000

[jmap.protocol.request]
max-concurrent = 8

[jmap.protocol.upload]
max-size = 5000000
max-concurrent = 4

[jmap.rate-limit]
account.rate = '100/1m'
authentication.rate = '100/1m'
anonymous.rate = '1000/1m'

[jmap.event-source]
throttle = '500ms'

[jmap.web-sockets]
throttle = '500ms'

[jmap.push]
throttle = '500ms'
attempts.interval = '500ms'

[jmap.auth.database]
type = 'sql'
address = 'sqlite::memory:'

[jmap.auth.database.query]
uid-by-login = 'SELECT ROWID - 1 FROM users WHERE login = ?'
login-by-uid = 'SELECT login FROM users WHERE ROWID - 1 = ?'
secret-by-uid = 'SELECT secret FROM users WHERE ROWID - 1 = ?'
gids-by-uid = 'SELECT gid FROM groups WHERE uid = ?'

[oauth]
key = 'parerga_und_paralipomena'
max-auth-attempts = 1

[oauth.expiry]
user-code = '1s'
token = '1s'
refresh-token = '3s'
refresh-token-renew = '2s'
";

#[tokio::test]
pub async fn jmap_tests() {
    tracing::subscriber::set_global_default(
        tracing_subscriber::FmtSubscriber::builder()
            .with_max_level(tracing::Level::WARN)
            .finish(),
    )
    .unwrap();

    let delete = true;
    let mut params = init_jmap_tests(delete).await;
    //email_query::test(params.server.clone(), &mut params.client, delete).await;
    //email_get::test(params.server.clone(), &mut params.client).await;
    //email_set::test(params.server.clone(), &mut params.client).await;
    //email_parse::test(params.server.clone(), &mut params.client).await;
    //email_search_snippet::test(params.server.clone(), &mut params.client).await;
    //email_changes::test(params.server.clone(), &mut params.client).await;
    //email_query_changes::test(params.server.clone(), &mut params.client).await;
    //email_copy::test(params.server.clone(), &mut params.client).await;
    //thread_get::test(params.server.clone(), &mut params.client).await;
    //thread_merge::test(params.server.clone(), &mut params.client).await;
    //mailbox::test(params.server.clone(), &mut params.client).await;
    //auth_acl::test(params.server.clone(), &mut params.client).await;
    //auth_limits::test(params.server.clone(), &mut params.client).await;
    //auth_oauth::test(params.server.clone(), &mut params.client).await;
    //event_source::test(params.server.clone(), &mut params.client).await;
    push_subscription::test(params.server.clone(), &mut params.client).await;

    if delete {
        params.temp_dir.delete();
    }
}

#[allow(dead_code)]
struct JMAPTest {
    server: Arc<JMAP>,
    client: Client,
    temp_dir: TempDir,
    shutdown_tx: watch::Sender<bool>,
}

async fn init_jmap_tests(delete_if_exists: bool) -> JMAPTest {
    // Load and parse config
    let temp_dir = TempDir::new("jmap_tests", delete_if_exists);
    let settings = utils::config::Config::parse(
        &add_test_certs(SERVER).replace("{TMP}", &temp_dir.path.display().to_string()),
    )
    .unwrap();
    let servers = settings.parse_servers().unwrap();

    // Start JMAP server
    servers.bind(&settings);
    let manager = JmapSessionManager::new(JMAP::init(&settings).await);
    let shutdown_tx = servers.spawn(|server, shutdown_rx| {
        server.spawn(manager.clone(), shutdown_rx);
    });

    // Create tables
    for query in [
        "CREATE TABLE users (login TEXT PRIMARY KEY, secret TEXT, name TEXT)",
        "CREATE TABLE groups (uid INTEGER, gid INTEGER, PRIMARY KEY (uid, gid))",
        "CREATE TABLE emails (uid INTEGER NOT NULL, email TEXT NOT NULL, PRIMARY KEY (uid, email))",
        "INSERT INTO users (login, secret) VALUES ('admin', 'secret')", // RowID 0 is admin
    ] {
        assert!(
            manager
                .inner
                .auth_db
                .execute(query, Vec::<String>::new().into_iter())
                .await,
            "failed for {query}"
        );
    }

    // Create client
    let mut client = Client::new()
        .credentials(Credentials::basic("admin", "secret"))
        .timeout(Duration::from_secs(60))
        .accept_invalid_certs(true)
        .connect("https://127.0.0.1:8899")
        .await
        .unwrap();
    client.set_default_account_id(Id::new(1));

    JMAPTest {
        server: manager.inner,
        temp_dir,
        client,
        shutdown_tx,
    }
}

pub fn find_values(string: &str, name: &str) -> Vec<String> {
    let mut last_pos = 0;
    let mut values = Vec::new();

    while let Some(pos) = string[last_pos..].find(name) {
        let mut value = string[last_pos + pos + name.len()..]
            .split('"')
            .nth(1)
            .unwrap();
        if value.ends_with('\\') {
            value = &value[..value.len() - 1];
        }
        values.push(value.to_string());
        last_pos += pos + name.len();
    }

    values
}

pub fn replace_values(mut string: String, find: &[String], replace: &[String]) -> String {
    for (find, replace) in find.iter().zip(replace.iter()) {
        string = string.replace(find, replace);
    }
    string
}

pub fn replace_boundaries(string: String) -> String {
    let values = find_values(&string, "boundary=");
    if !values.is_empty() {
        replace_values(
            string,
            &values,
            &(0..values.len())
                .map(|i| format!("boundary_{}", i))
                .collect::<Vec<_>>(),
        )
    } else {
        string
    }
}

pub fn replace_blob_ids(string: String) -> String {
    let values = find_values(&string, "blobId\":");
    if !values.is_empty() {
        replace_values(
            string,
            &values,
            &(0..values.len())
                .map(|i| format!("blob_{}", i))
                .collect::<Vec<_>>(),
        )
    } else {
        string
    }
}

pub async fn test_account_create(jmap: &JMAP, login: &str, secret: &str, name: &str) -> Id {
    assert!(
        jmap.auth_db
            .execute(
                "INSERT OR REPLACE INTO users (login, secret, name) VALUES (?, ?, ?)",
                vec![login.to_string(), secret.to_string(), name.to_string()].into_iter()
            )
            .await
    );
    Id::new(jmap.get_account_id(login).await.unwrap() as u64)
}

pub async fn test_account_login(login: &str, secret: &str) -> Client {
    Client::new()
        .credentials(Credentials::basic(login, secret))
        .timeout(Duration::from_secs(5))
        .accept_invalid_certs(true)
        .connect("https://127.0.0.1:8899")
        .await
        .unwrap()
}