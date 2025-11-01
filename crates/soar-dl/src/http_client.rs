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

    pub fn get<T>(&self, uri: T) -> RequestBuilder<WithoutBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE.read().unwrap();
        let req = state.agent.get(uri);
        apply_headers(req, &state.config.headers)
    }

    pub fn post<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE.read().unwrap();
        let req = state.agent.post(uri);
        apply_headers(req, &state.config.headers)
    }

    pub fn put<T>(&self, uri: T) -> RequestBuilder<WithBody>
    where
        Uri: TryFrom<T>,
        <Uri as TryFrom<T>>::Error: Into<http::Error>,
    {
        let state = SHARED_CLIENT_STATE.read().unwrap();
        let req = state.agent.put(uri);
        apply_headers(req, &state.config.headers)
    }

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

fn apply_headers<B>(mut req: RequestBuilder<B>, headers: &Option<HeaderMap>) -> RequestBuilder<B> {
    if let Some(headers) = headers {
        for (key, value) in headers.iter() {
            req = req.header(key, value);
        }
    }
    req
}

pub static SHARED_AGENT: LazyLock<SharedAgent> = LazyLock::new(SharedAgent::new);

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
