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
    /// use soar_dl::http_client::ClientConfig;
    ///
    /// let cfg = ClientConfig::default();
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
    /// use soar_dl::http_client::ClientConfig;
    ///
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
        let state = SHARED_CLIENT_STATE.read().unwrap();
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
    /// use soar_dl::http_client::SHARED_AGENT;
    ///
    /// let req = SHARED_AGENT.post("https://example.com/");
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
    /// use soar_dl::http_client::SHARED_AGENT;
    ///
    /// let req = SHARED_AGENT.put("https://example.com/resource");
    /// ```
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
    let mut state = SHARED_CLIENT_STATE.write().unwrap();
    let mut new_config = state.config.clone();
    updater(&mut new_config);
    let new_agent = new_config.build();
    state.agent = new_agent;
    state.config = new_config;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert_eq!(config.user_agent, Some("pkgforge/soar".to_string()));
        assert!(config.proxy.is_none());
        assert!(config.headers.is_none());
        assert!(config.timeout.is_none());
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
        };
        let agent = config.build();
        let _ = agent;
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
        let agent = SharedAgent::default();
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
