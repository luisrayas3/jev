use crate::label::{
    Classification, Integrity, Labeled,
    SatisfiesClassification, SatisfiesIntegrity,
};
use crate::runtime::RuntimeKey;
use anyhow::Result;
use std::marker::PhantomData;

pub const API_DOCS: &str =
    r#"## HTTP resource - `jevs::Http`

```rust
// GET returns labeled data
let resp = res.api.get("/endpoint").await?;

// POST with labeled body
let result = res.api.post("/endpoint", body).await?;

// Set headers (e.g. auth tokens)
res.api.set_header("Authorization", "Bearer ...");
```

Key: both `get` and `post` take `&self`.
Parallel GETs are fine via `tokio::join!`.
`set_header` takes `&mut self` (setup before requests).

Do NOT construct Http yourself.
It is provided via `res.<name>`.
"#;

pub struct Http<C: Classification, I: Integrity> {
    client: reqwest::Client,
    base: String,
    headers: Vec<(String, String)>,
    _c: PhantomData<C>,
    _i: PhantomData<I>,
}

impl<C: Classification, I: Integrity> Http<C, I> {
    pub fn open(_key: &RuntimeKey, base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("jev/0.1")
            .build()
            .expect("failed to build HTTP client");
        Http {
            client,
            base: base_url.to_string(),
            headers: Vec::new(),
            _c: PhantomData,
            _i: PhantomData,
        }
    }

    pub fn set_header(&mut self, name: &str, value: &str) {
        self.headers
            .push((name.to_string(), value.to_string()));
    }

    pub async fn get(
        &self,
        path: &str,
    ) -> Result<Labeled<String, C, I>> {
        let url = self.url(path);
        let rb = self.request(self.client.get(&url));
        let text = rb
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(Labeled::new(text))
    }

    pub async fn post<Cb: Classification, Ib: Integrity>(
        &self,
        path: &str,
        body: Labeled<String, Cb, Ib>,
    ) -> Result<Labeled<String, C, I>>
    where
        Cb: SatisfiesClassification<C>,
        Ib: SatisfiesIntegrity<I>,
    {
        let url = self.url(path);
        let rb = self
            .request(self.client.post(&url))
            .body(body.into_inner());
        let text = rb
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Ok(Labeled::new(text))
    }

    fn url(&self, path: &str) -> String {
        let base = self.base.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        if path.is_empty() {
            base.to_string()
        } else {
            format!("{base}/{path}")
        }
    }

    fn request(
        &self,
        mut rb: reqwest::RequestBuilder,
    ) -> reqwest::RequestBuilder {
        for (name, value) in &self.headers {
            rb = rb.header(name, value);
        }
        rb
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::label::{Me, Public};

    fn test_http(base: &str) -> Http<Public, Me> {
        Http {
            client: reqwest::Client::new(),
            base: base.to_string(),
            headers: Vec::new(),
            _c: PhantomData,
            _i: PhantomData,
        }
    }

    #[test]
    fn url_join_simple() {
        let h = test_http("https://example.com");
        assert_eq!(h.url("/foo"), "https://example.com/foo");
    }

    #[test]
    fn url_join_trailing_slash() {
        let h = test_http("https://example.com/");
        assert_eq!(h.url("/bar"), "https://example.com/bar");
    }

    #[test]
    fn url_join_no_leading_slash() {
        let h = test_http("https://example.com");
        assert_eq!(h.url("baz"), "https://example.com/baz");
    }

    #[test]
    fn url_join_empty_path() {
        let h = test_http("https://example.com/api");
        assert_eq!(h.url(""), "https://example.com/api");
    }

    #[test]
    fn url_join_both_slashes() {
        let h = test_http("https://example.com/");
        assert_eq!(h.url("/a/b"), "https://example.com/a/b");
    }

    #[test]
    fn set_header_stores() {
        let mut h = test_http("https://example.com");
        h.set_header("Authorization", "Bearer tok");
        h.set_header("X-Custom", "val");
        assert_eq!(h.headers.len(), 2);
        assert_eq!(h.headers[0].0, "Authorization");
        assert_eq!(h.headers[1].1, "val");
    }
}
