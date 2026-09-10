use super::*;

use gateway_core::egress::{AccountProxyRoute, ProxyEndpoint};
use std::net::SocketAddr;
use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

struct SocksProxy {
    address: SocketAddr,
    targets: Arc<Mutex<Vec<String>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for SocksProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl SocksProxy {
    async fn start(target: SocketAddr, authenticate: bool, reject: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let targets = Arc::new(Mutex::new(Vec::new()));
        let seen = targets.clone();
        let task = tokio::spawn(async move {
            let mut tunnels = tokio::task::JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let (mut stream,_) = accepted.unwrap();
                        let seen = seen.clone();
                        tunnels.spawn(async move {
                            let mut greeting = [0;2]; stream.read_exact(&mut greeting).await.unwrap();
                            assert_eq!(greeting[0],5);
                            let mut methods=vec![0;greeting[1] as usize]; stream.read_exact(&mut methods).await.unwrap();
                            if reject { stream.write_all(&[5,255]).await.unwrap(); return; }
                            stream.write_all(&[5,if authenticate {2} else {0}]).await.unwrap();
                            if authenticate {
                                assert_eq!(stream.read_u8().await.unwrap(),1);
                                let len=stream.read_u8().await.unwrap() as usize;
                                let mut user=vec![0;len];stream.read_exact(&mut user).await.unwrap();
                                let len=stream.read_u8().await.unwrap() as usize;
                                let mut password=vec![0;len];stream.read_exact(&mut password).await.unwrap();
                                assert_eq!(user,b"user");assert_eq!(password,b"p@ss");
                                stream.write_all(&[1,0]).await.unwrap();
                            }
                            let mut request=[0;4];stream.read_exact(&mut request).await.unwrap();
                            assert_eq!(&request[..3],&[5,1,0]);
                            let host=match request[3] {
                                3 => {let n=stream.read_u8().await.unwrap() as usize;let mut b=vec![0;n];stream.read_exact(&mut b).await.unwrap();String::from_utf8(b).unwrap()},
                                1 => {let mut b=[0;4];stream.read_exact(&mut b).await.unwrap();std::net::Ipv4Addr::from(b).to_string()},
                                4 => {let mut b=[0;16];stream.read_exact(&mut b).await.unwrap();std::net::Ipv6Addr::from(b).to_string()},
                                other => panic!("unexpected address type {other}"),
                            };
                            let port=stream.read_u16().await.unwrap();
                            seen.lock().unwrap().push(format!("{host}:{port}"));
                            let mut upstream=TcpStream::connect(target).await.unwrap();
                            stream.write_all(&[5,0,0,1,127,0,0,1,0,0]).await.unwrap();
                            let _=tokio::io::copy_bidirectional(&mut stream,&mut upstream).await;
                        });
                    },
                    result=tunnels.join_next(), if !tunnels.is_empty() => { if let Some(Err(error))=result { panic!("proxy task: {error}"); } }
                }
            }
        });
        Self {
            address,
            targets,
            task,
        }
    }

    fn route(&self, key: &str, authenticate: bool) -> AccountProxyRoute {
        AccountProxyRoute {
            key: key.to_owned(),
            endpoint: ProxyEndpoint::parse(&format!(
                "socks5h://{}{}",
                if authenticate { "user:p%40ss@" } else { "" },
                self.address
            ))
            .unwrap(),
        }
    }
}

#[tokio::test]
async fn http_accounts_use_distinct_authenticated_socks_exits_and_remote_dns() {
    let one = MockServer::start().await;
    let two = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"exit":"one","rate_limit":{}})),
        )
        .mount(&one)
        .await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"exit":"two","rate_limit":{}})),
        )
        .mount(&two)
        .await;
    let a = SocksProxy::start(*one.address(), true, false).await;
    let b = SocksProxy::start(*two.address(), false, false).await;
    let backend = CodexBackendClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://upstream.invalid:8080",
        test_wire_profile(),
    );
    for (proxy, auth, expected) in [(&a, true, "one"), (&b, false, "two"), (&a, true, "one")] {
        let client = backend
            .with_proxy(Some(&proxy.route(expected, auth)))
            .unwrap();
        let result = timeout(
            Duration::from_secs(5),
            client.fetch_usage(request_context("egress", Some("account"))),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result["exit"], expected);
        assert!(
            proxy
                .targets
                .lock()
                .unwrap()
                .iter()
                .all(|h| h == "upstream.invalid:8080")
        );
        assert!(!proxy.targets.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn unavailable_socks_never_falls_back_to_direct_http() {
    let upstream = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"leaked":true,"rate_limit":{}})),
        )
        .mount(&upstream)
        .await;
    let proxy = SocksProxy::start(*upstream.address(), false, true).await;
    let backend = CodexBackendClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        upstream.uri(),
        test_wire_profile(),
    )
    .with_proxy(Some(&proxy.route("reject", false)))
    .unwrap();
    assert!(
        timeout(
            Duration::from_secs(5),
            backend.fetch_usage(request_context("egress", None))
        )
        .await
        .unwrap()
        .is_err()
    );
    let mut request = codex_request("gpt-5.5", "test", Vec::new());
    request.set_previous_response_id(Some("resp_test".to_owned()));
    request.previous_response_scope = Some(PreviousResponseScope::Persisted);
    assert!(
        timeout(
            Duration::from_secs(5),
            backend.create_response(&request, request_context("egress", Some("account")))
        )
        .await
        .unwrap()
        .is_err()
    );
    assert!(upstream.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn websocket_pool_does_not_reuse_a_connection_after_proxy_change() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut sockets = Vec::new();
        for id in ["resp_proxy_one", "resp_proxy_two"] {
            let (socket, _) = listener.accept().await.unwrap();
            let mut ws = accept_codex_test_websocket(socket).await;
            ws.next().await.unwrap().unwrap();
            ws.send(Message::Text(completed_websocket_response(id, 3, 1).into()))
                .await
                .unwrap();
            sockets.push(ws);
        }
    });
    let a = SocksProxy::start(address, true, false).await;
    let b = SocksProxy::start(address, false, false).await;
    let pool = Arc::new(CodexWebSocketPool::new(Duration::from_secs(60)));
    let backend = CodexBackendClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        "http://upstream.invalid:8080",
        test_wire_profile(),
    )
    .with_websocket_pool(pool.clone());
    let mut request = codex_request_with_prompt_cache_key(
        "gpt-5.5",
        "be brief",
        Vec::new(),
        "proxy-conversation",
    );
    request.set_previous_response_id(Some("resp_previous".to_owned()));
    request.previous_response_scope = Some(PreviousResponseScope::Persisted);
    for (proxy, auth, key) in [
        (&a, true, "account:proxy:1"),
        (&b, false, "account:proxy:2"),
    ] {
        let backend = backend.with_proxy(Some(&proxy.route(key, auth))).unwrap();
        if key.ends_with(":2") {
            let mut continuation = request.clone();
            continuation.set_previous_response_id(Some("resp_proxy_one".to_owned()));
            continuation.previous_response_scope = Some(PreviousResponseScope::ConnectionLocal);
            assert!(
                timeout(
                    Duration::from_secs(5),
                    backend.create_response(
                        &continuation,
                        request_context("egress", Some("same-account"))
                    )
                )
                .await
                .unwrap()
                .is_err()
            );
            assert!(proxy.targets.lock().unwrap().is_empty());
        }
        let result = timeout(
            Duration::from_secs(5),
            backend.create_response(&request, request_context("egress", Some("same-account"))),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result.transport, CodexBackendTransport::WebSocket);
        assert_eq!(
            proxy.targets.lock().unwrap().as_slice(),
            &["upstream.invalid:8080"]
        );
    }
    server.await.unwrap();
    pool.shutdown().await;
}

#[tokio::test]
async fn token_refresh_uses_bound_proxy_and_never_direct_fallback() {
    use gateway_core::account::ProviderAccountId;
    use provider_openai::credential::token_client::{
        OpenAiTokenClient, TokenClientConfig, TokenRefresher,
    };
    let upstream = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"access_token":"fake-new-token"})),
        )
        .mount(&upstream)
        .await;
    let proxy = SocksProxy::start(*upstream.address(), true, false).await;
    let store = Arc::new(crate::support::MemoryAccountStore::default());
    let account = ProviderAccountId::new("acct_proxy_refresh").unwrap();
    store.bind_proxy(account.clone(), proxy.route("refresh:1", true));
    let client = OpenAiTokenClient::new(
        reqwest::Client::builder().no_proxy().build().unwrap(),
        TokenClientConfig {
            client_id: "test".to_owned(),
            token_endpoint: format!("{}/oauth/token", upstream.uri()),
        },
        test_wire_profile(),
    )
    .with_accounts(store.clone());
    let result = timeout(
        Duration::from_secs(5),
        client.refresh_for_account(&account, "fake-refresh-token"),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result.access_token.as_deref(), Some("fake-new-token"));
    assert_eq!(proxy.targets.lock().unwrap().len(), 1);
    let failed = SocksProxy::start(*upstream.address(), false, true).await;
    store.bind_proxy(account.clone(), failed.route("refresh:2", false));
    assert!(
        timeout(
            Duration::from_secs(5),
            client.refresh_for_account(&account, "fake-refresh-token")
        )
        .await
        .unwrap()
        .is_err()
    );
    assert_eq!(upstream.received_requests().await.unwrap().len(), 1);
}
