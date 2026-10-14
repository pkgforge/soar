use std::{
    sync::{Arc, LazyLock, PoisonError, RwLock},
    time::Duration,
};

use ureq::{
    Agent, Proxy, RequestBuilder, Timeout,
    config::IpFamily,
    http::{self, HeaderMap, Uri},
    typestate::{WithBody, WithoutBody},
    unversioned::{
        resolver::DefaultResolver,
        transport::{
            Buffers, ConnectionDetails, Connector, DefaultConnector, NextTimeout, Transport,
        },
    },
};

/// Bounds TCP connect and TLS handshake, so an unroutable address fails over to the next one
/// instead of stalling forever.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Bounds how long a request waits for the next bytes from the server, so one that stops
/// sending fails the read instead of hanging. The clock restarts whenever data arrives, so
/// it does not cap how long a large transfer may take.
///
/// ureq's own receive timeouts each cap a whole phase, the body included, so the bound is
/// applied by [`StallConnector`] instead.
const STALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Wraps each connection ureq makes in a [`StallTransport`].
///
/// This uses ureq's `unversioned` transport API, which may change in a ureq minor release.
#[derive(Debug)]
struct StallConnector(Duration);

impl Connector<Box<dyn Transport>> for StallConnector {
    type Out = StallTransport;

    fn connect(
        &self,
        _details: &ConnectionDetails,
        chained: Option<Box<dyn Transport>>,
    ) -> Result<Option<Self::Out>, ureq::Error> {
        Ok(chained.map(|inner| {
            StallTransport {
                inner,
                stall: self.0,
            }
        }))
    }
}

/// A connection whose every wait for input gives up after `stall` without data.
#[derive(Debug)]
struct StallTransport {
    inner: Box<dyn Transport>,
    stall: Duration,
}

impl Transport for StallTransport {
    fn buffers(&mut self) -> &mut dyn Buffers {
        self.inner.buffers()
    }

    fn transmit_output(&mut self, amount: usize, timeout: NextTimeout) -> Result<(), ureq::Error> {
        self.inner.transmit_output(amount, timeout)
    }

    fn maybe_await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let timeout = self.cap(timeout);
        self.inner.maybe_await_input(timeout)
    }

    fn await_input(&mut self, timeout: NextTimeout) -> Result<bool, ureq::Error> {
        let timeout = self.cap(timeout);
        self.inner.await_input(timeout)
    }

    fn is_open(&mut self) -> bool {
        self.inner.is_open()
    }

    fn is_tls(&self) -> bool {
        self.inner.is_tls()
    }
}

impl StallTransport {
    fn cap(&self, timeout: NextTimeout) -> NextTimeout {
        let stall = self.stall.into();
        if stall < timeout.after {
            NextTimeout {
                after: stall,
                reason: Timeout::RecvBody,
            }
        } else {
            timeout
        }
    }
}

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub user_agent: Option<String>,
    pub headers: Option<HeaderMap>,
    pub proxy: Option<Proxy>,
    pub timeout: Option<Duration>,
    pub ip_family: IpFamily,
}

impl Default for ClientConfig {
    /// Creates a default ClientConfig populated with sensible defaults for HTTP requests.
    ///
    /// The default sets a user agent of "pkgforge/soar", allows both IPv4 and IPv6, and leaves
    /// proxy, headers, and timeout unset.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::http_client::ClientConfig;
    /// use ureq::config::IpFamily;
    ///
    /// let cfg = ClientConfig::default();
    /// assert_eq!(cfg.user_agent.as_deref(), Some("pkgforge/soar"));
    /// assert!(cfg.proxy.is_none());
    /// assert!(cfg.headers.is_none());
    /// assert!(cfg.timeout.is_none());
    /// assert_eq!(cfg.ip_family, IpFamily::Any);
    /// ```
    fn default() -> Self {
        Self {
            user_agent: Some("pkgforge/soar".into()),
            proxy: None,
            headers: None,
            timeout: None,
            ip_family: IpFamily::Any,
        }
    }
}

impl ClientConfig {
    /// Builds an HTTP `Agent` configured from this `ClientConfig`.
    ///
    /// The returned `Agent` will incorporate the configured proxy, global timeout,
    /// IP family, and user agent header (if present). A server that sends nothing for
    /// two minutes fails the request, whatever the global timeout.
    ///
    /// When no proxy is set, the `ALL_PROXY`, `HTTPS_PROXY`, `HTTP_PROXY` and `NO_PROXY`
    /// environment variables are honored.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::http_client::ClientConfig;
    ///
    /// let config = ClientConfig::default();
    /// let agent = config.build();
    /// // create a request builder using the configured agent
    /// let _req = agent.get("http://example.com");
    /// ```
    pub fn build(&self) -> Agent {
        self.build_with_stall(STALL_TIMEOUT)
    }

    fn build_with_stall(&self, stall: Duration) -> Agent {
        let mut config = ureq::Agent::config_builder()
            .timeout_global(self.timeout)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .ip_family(self.ip_family);

        if self.proxy.is_some() {
            config = config.proxy(self.proxy.clone());
        }

        if let Some(user_agent) = &self.user_agent {
            config = config.user_agent(user_agent);
        }

        Agent::with_parts(
            config.build(),
            DefaultConnector::default().chain(StallConnector(stall)),
            DefaultResolver::default(),
        )
    }
}

struct SharedClient {
    agent: Agent,
    config: ClientConfig,
}

static SHARED_CLIENT_STATE: LazyLock<Arc<RwLock<SharedClient>>> = LazyLock::new(|| {
    let config = ClientConfig::default();
    let agent = config.build();

    Arc::new(RwLock::new(SharedClient {
        agent,
        config,
    }))
});

#[derive(Clone, Default)]
pub struct SharedAgent;

impl SharedAgent {
    /// Create a new `SharedAgent` instance.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::http_client::SharedAgent;
    ///
    /// let _agent = SharedAgent::new();
    /// ```
    pub fn new() -> Self {
        Self
    }

    pub fn head<T>(&self, uri: T) -> RequestBuilder<WithoutBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let req = state.agent.head(uri);
        apply_headers(req, &state.config.headers)
    }

    /// Create a GET request builder for the given URI using the shared agent.
    ///
    /// The returned `RequestBuilder` does not contain a body; any global headers
    /// configured in the shared client are applied to the request.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::http_client::SHARED_AGENT;
    ///
    /// // Create and send a GET request to the specified URI.
    /// let response = SHARED_AGENT.get("https://example.com").call();
    /// ```
    pub fn get<T>(&self, uri: T) -> RequestBuilder<WithoutBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let req = state.agent.get(uri);
        apply_headers(req, &state.config.headers)
    }

    /// Starts a POST request to the given URI using the shared agent and applies any globally configured headers.
    ///
    /// The returned `RequestBuilder<WithBody>` is ready to accept a request body and further per-request modifications.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use soar_dl::http_client::SHARED_AGENT;
    ///
    /// let req = SHARED_AGENT.post("https://example.com/");
    /// ```
    pub fn post<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let req = state.agent.post(uri);
        apply_headers(req, &state.config.headers)
    }

    /// Creates a PUT request builder for the specified URI using the shared agent and applies any configured global headers.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::http_client::SHARED_AGENT;
    ///
    /// let req = SHARED_AGENT.put("https://example.com/resource");
    /// ```
    pub fn put<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let req = state.agent.put(uri);
        apply_headers(req, &state.config.headers)
    }

    /// Creates a DELETE request for the given URI using the shared agent and applies configured global headers.
    ///
    /// # Returns
    ///
    /// A `RequestBuilder<WithoutBody>` for the DELETE request with any configured global headers applied.
    ///
    /// # Examples
    ///
    /// ```
    /// use soar_dl::http_client::SharedAgent;
    ///
    /// let agent = SharedAgent::new();
    /// let _req = agent.delete("https://example.com/resource");
    /// ```
    pub fn delete<T>(&self, uri: T) -> RequestBuilder<WithoutBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let req = state.agent.delete(uri);
        apply_headers(req, &state.config.headers)
    }
}

/// Apply headers from an optional `HeaderMap` to a `RequestBuilder`.
///
/// If `headers` is `Some`, each header key/value pair is added to the provided request
/// and the modified `RequestBuilder` is returned. If `headers` is `None`, the original
/// request is returned unchanged.
fn apply_headers<B>(mut req: RequestBuilder<B>, headers: &Option<HeaderMap>) -> RequestBuilder<B> {
    if let Some(headers) = headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }
    req
}

pub static SHARED_AGENT: LazyLock<SharedAgent> = LazyLock::new(SharedAgent::new);

/// Updates the global shared HTTP client configuration by applying the provided updater and rebuilding the shared Agent.
///
/// The `updater` closure receives a mutable reference to a `ClientConfig` that will replace the current shared configuration.
/// After the updater runs, a new `Agent` is built from the updated config and atomically replaces the shared agent and config.
///
/// # Examples
///
/// ```
/// use soar_dl::http_client::configure_http_client;
///
/// // Change the global user agent string used by the shared HTTP client.
/// configure_http_client(|cfg| {
///     cfg.user_agent = Some("my-app/1.0".to_string());
/// });
/// ```
pub fn configure_http_client<F>(updater: F)
where
    F: FnOnce(&mut ClientConfig),
{
    let mut state = SHARED_CLIENT_STATE
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    let mut new_config = state.config.clone();
    updater(&mut new_config);
    let new_agent = new_config.build();
    state.agent = new_agent;
    state.config = new_config;
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
        time::Instant,
    };

    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert_eq!(config.user_agent, Some("pkgforge/soar".to_string()));
        assert!(config.proxy.is_none());
        assert!(config.headers.is_none());
        assert!(config.timeout.is_none());
        assert_eq!(config.ip_family, IpFamily::Any);
    }

    #[test]
    fn test_client_config_build() {
        let config = ClientConfig::default();
        let agent = config.build();
        // Just verify it builds without panicking
        let _ = agent;
    }

    #[test]
    fn test_client_config_with_timeout() {
        let config = ClientConfig {
            user_agent: Some("test-agent".to_string()),
            proxy: None,
            headers: None,
            timeout: Some(Duration::from_secs(30)),
            ip_family: IpFamily::Any,
        };
        let agent = config.build();
        let _ = agent;
    }

    #[test]
    fn test_client_config_sets_connect_timeout() {
        let agent = ClientConfig::default().build();
        assert_eq!(agent.config().timeouts().connect, Some(CONNECT_TIMEOUT));
    }

    #[test]
    fn test_client_config_bounds_stalled_reads() {
        let agent = ClientConfig::default().build();
        let timeouts = agent.config().timeouts();
        assert_eq!(timeouts.recv_response, None);
        assert_eq!(timeouts.recv_body, None);
        assert_eq!(timeouts.global, None);
    }

    /// Serves one response whose body is `chunks`, sleeping `gap` before each.
    fn serve_slowly(chunks: usize, gap: Duration) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf);
            let head = format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", chunks + 1);
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(b"x");
            for _ in 0..chunks {
                thread::sleep(gap);
                if stream.write_all(b"x").is_err() {
                    return;
                }
            }
        });
        format!("http://{addr}/")
    }

    #[test]
    fn test_stalled_body_fails_within_bound() {
        let agent = ClientConfig::default().build_with_stall(Duration::from_millis(300));
        let url = serve_slowly(1, Duration::from_secs(5));

        let started = Instant::now();
        let result = agent.get(&url).call().unwrap().body_mut().read_to_vec();
        assert!(result.is_err(), "a stalled body must fail the read");
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[derive(Debug)]
    struct FakeTls;

    impl Transport for FakeTls {
        fn buffers(&mut self) -> &mut dyn Buffers {
            unimplemented!()
        }

        fn transmit_output(&mut self, _: usize, _: NextTimeout) -> Result<(), ureq::Error> {
            unimplemented!()
        }

        fn await_input(&mut self, _: NextTimeout) -> Result<bool, ureq::Error> {
            unimplemented!()
        }

        fn is_open(&mut self) -> bool {
            true
        }

        fn is_tls(&self) -> bool {
            true
        }
    }

    #[test]
    fn test_stall_transport_keeps_tls() {
        let transport = StallTransport {
            inner: Box::new(FakeTls),
            stall: STALL_TIMEOUT,
        };
        assert!(transport.is_tls(), "https requests would be refused");
    }

    #[test]
    fn test_silent_server_fails_within_bound() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_secs(5));
        });
        let agent = ClientConfig::default().build_with_stall(Duration::from_millis(300));

        let started = Instant::now();
        assert!(agent.get(format!("http://{addr}/")).call().is_err());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn test_slow_body_outlasts_stall_bound() {
        let agent = ClientConfig::default().build_with_stall(Duration::from_secs(1));
        let url = serve_slowly(10, Duration::from_millis(150));

        let body = agent.get(&url).call().unwrap().body_mut().read_to_vec();
        assert_eq!(body.unwrap().len(), 11);
    }

    #[test]
    fn test_client_config_without_proxy_falls_back_to_env() {
        let agent = ClientConfig::default().build();
        assert_eq!(
            agent.config().proxy().is_some(),
            Proxy::try_from_env().is_some()
        );
    }

    #[test]
    fn test_client_config_explicit_proxy_overrides_env() {
        let config = ClientConfig {
            proxy: Some(Proxy::new("http://127.0.0.1:8080").unwrap()),
            ..Default::default()
        };
        let agent = config.build();
        assert_eq!(agent.config().proxy().unwrap().port(), 8080);
    }

    #[test]
    fn test_client_config_ip_family() {
        for family in [IpFamily::Any, IpFamily::Ipv4Only, IpFamily::Ipv6Only] {
            let config = ClientConfig {
                ip_family: family,
                ..Default::default()
            };
            let agent = config.build();
            assert_eq!(agent.config().ip_family(), family);
        }
    }

    #[test]
    fn test_shared_agent_new() {
        let agent = SharedAgent::new();
        let _ = agent;
    }

    #[test]
    fn test_shared_agent_get() {
        let agent = SharedAgent::new();
        let req = agent.get("https://example.com");
        // Verify the request builder was created
        let _ = req;
    }

    #[test]
    fn test_shared_agent_post() {
        let agent = SharedAgent::new();
        let req = agent.post("https://example.com");
        let _ = req;
    }

    #[test]
    fn test_shared_agent_put() {
        let agent = SharedAgent::new();
        let req = agent.put("https://example.com");
        let _ = req;
    }

    #[test]
    fn test_shared_agent_delete() {
        let agent = SharedAgent::new();
        let req = agent.delete("https://example.com");
        let _ = req;
    }

    #[test]
    fn test_shared_agent_head() {
        let agent = SharedAgent::new();
        let req = agent.head("https://example.com");
        let _ = req;
    }

    #[test]
    fn test_configure_http_client() {
        configure_http_client(|cfg| {
            cfg.user_agent = Some("custom-agent/1.0".to_string());
        });

        // Verify configuration was applied by checking we can still create requests
        let agent = SharedAgent::new();
        let _ = agent.get("https://example.com");
    }

    #[test]
    fn test_configure_http_client_timeout() {
        configure_http_client(|cfg| {
            cfg.timeout = Some(Duration::from_secs(10));
        });

        let agent = SharedAgent::new();
        let _ = agent.get("https://example.com");
    }

    #[test]
    fn test_shared_agent_clone() {
        let agent1 = SharedAgent::new();
        let agent2 = agent1.clone();

        // Both should work
        let _ = agent1.get("https://example.com");
        let _ = agent2.get("https://example.com");
    }

    #[test]
    fn test_shared_agent_default() {
        let agent = SharedAgent;
        let _ = agent.get("https://example.com");
    }

    #[test]
    fn test_apply_headers_none() {
        let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
        let req = agent.get("https://example.com");
        let req = apply_headers(req, &None);
        let _ = req;
    }

    #[test]
    fn test_apply_headers_some() {
        let agent: ureq::Agent = ureq::Agent::config_builder().build().into();
        let req = agent.get("https://example.com");

        let mut headers = ureq::http::HeaderMap::new();
        headers.insert(
            ureq::http::header::USER_AGENT,
            ureq::http::HeaderValue::from_static("test-agent"),
        );

        let req = apply_headers(req, &Some(headers));
        let _ = req;
    }

    #[test]
    fn test_client_config_clone() {
        let config1 = ClientConfig::default();
        let config2 = config1.clone();

        assert_eq!(config1.user_agent, config2.user_agent);
    }

    #[test]
    fn test_client_config_debug() {
        let config = ClientConfig::default();
        let debug = format!("{:?}", config);
        assert!(debug.contains("ClientConfig"));
    }
}
