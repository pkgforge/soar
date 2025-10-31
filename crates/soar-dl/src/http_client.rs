use std::{
    sync::{Arc, LazyLock, RwLock},
    time::Duration,
};

use ureq::{
    http::{self, HeaderMap, Uri},
    typestate::{WithBody, WithoutBody},
    Agent, Proxy, RequestBuilder,
};

#[derive(Clone, Debug)]
pub struct ClientConfig {
    pub user_agent: Option<String>,
    pub headers: Option<HeaderMap>,
    pub proxy: Option<Proxy>,
    pub timeout: Option<Duration>,
}

impl Default for ClientConfig {
    /// Creates a default ClientConfig populated with sensible defaults for HTTP requests.
    ///
    /// The default sets a user agent of "pkgforge/soar" and leaves proxy, headers, and timeout unset.
    ///
    /// # Examples
    ///
    /// ```
    /// let cfg = crates::soar_dl::http_client::ClientConfig::default();
    /// assert_eq!(cfg.user_agent.as_deref(), Some("pkgforge/soar"));
    /// assert!(cfg.proxy.is_none());
    /// assert!(cfg.headers.is_none());
    /// assert!(cfg.timeout.is_none());
    /// ```
    fn default() -> Self {
        Self {
            user_agent: Some("pkgforge/soar".into()),
            proxy: None,
            headers: None,
            timeout: None,
        }
    }
}

impl ClientConfig {
    /// Builds an HTTP `Agent` configured from this `ClientConfig`.
    ///
    /// The returned `Agent` will incorporate the configured proxy, global timeout,
    /// and user agent header (if present).
    ///
    /// # Examples
    ///
    /// ```
    /// let config = ClientConfig::default();
    /// let agent = config.build();
    /// // create a request builder using the configured agent
    /// let _req = agent.get("http://example.com");
    /// ```
    pub fn build(&self) -> Agent {
        let mut config = ureq::Agent::config_builder()
            .proxy(self.proxy.clone())
            .timeout_global(self.timeout);

        if let Some(user_agent) = &self.user_agent {
            config = config.user_agent(user_agent);
        }

        config.build().into()
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
    /// use crate::http_client::SharedAgent;
    ///
    /// let _agent = SharedAgent::new();
    /// ```
    pub fn new() -> Self {
        Self
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
        let state = SHARED_CLIENT_STATE.read().unwrap();
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
    /// let req = crate::SHARED_AGENT.post("https://example.com/");
    /// let req = req.send_string("payload"); // sends the request with a string body
    /// ```
    pub fn post<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE.read().unwrap();
        let req = state.agent.post(uri);
        apply_headers(req, &state.config.headers)
    }

    /// Creates a PUT request builder for the specified URI using the shared agent and applies any configured global headers.
    ///
    /// # Examples
    ///
    /// ```
    /// let req = SHARED_AGENT.put("https://example.com/resource");
    /// // `req` is a `RequestBuilder<ureq::WithBody>` ready to have a body set and be sent:
    /// // let resp = req.send_string("payload");
    /// ```
    pub fn put<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
    Uri: TryFrom<T>,
    <Uri as TryFrom<T>>::Error: Into<http::Error>,
    pub fn put<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE.read().unwrap();
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
    /// let agent = SharedAgent::new();
    /// let _req = agent.delete("https://example.com/resource");
    /// ```
    pub fn delete<T>(&self, uri: T) -> RequestBuilder<WithoutBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE.read().unwrap();
        let req = state.agent.delete(uri);
        apply_headers(req, &state.config.headers)
    }
}

/// Apply headers from an optional `HeaderMap` to a `RequestBuilder`.
///
/// If `headers` is `Some`, each header key/value pair is added to the provided request
/// and the modified `RequestBuilder` is returned. If `headers` is `None`, the original
/// request is returned unchanged.
///
/// # Examples
///
/// ```
/// use http::HeaderMap;
/// use ureq::Agent;
///
/// // create headers
/// let mut headers = HeaderMap::new();
/// headers.insert("x-test".parse().unwrap(), "v".parse().unwrap());
///
/// let agent = Agent::new();
/// let req = agent.get("http://example.com");
/// let req = apply_headers(req, &Some(headers));
/// // `req` now contains the `x-test: v` header
/// ```
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
/// // Change the global user agent string used by the shared HTTP client.
/// configure_http_client(|cfg| {
///     cfg.user_agent = Some("my-app/1.0".to_string());
/// });
/// ```
pub fn configure_http_client<F>(updater: F)
where
    F: FnOnce(&mut ClientConfig),
{
    let mut state = SHARED_CLIENT_STATE.write().unwrap();
    let mut new_config = state.config.clone();
    updater(&mut new_config);
    let new_agent = new_config.build();
    state.agent = new_agent;
    state.config = new_config;
}