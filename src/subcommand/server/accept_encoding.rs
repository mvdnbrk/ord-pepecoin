use {axum::http, http::StatusCode};

/// Extracts the raw `Accept-Encoding` header value, if present.
pub(crate) struct AcceptEncoding(pub(crate) Option<String>);

impl AcceptEncoding {
  /// Returns true if the client accepts the given encoding (e.g. "br", "gzip"),
  /// or if no Content-Encoding is set on the inscription (identity is always accepted).
  pub(crate) fn accepts(&self, encoding: Option<&str>) -> bool {
    let Some(encoding) = encoding else {
      return true;
    };

    match &self.0 {
      Some(accept) => accept.split(',').any(|s| {
        let s = s.trim().split(';').next().unwrap_or("").trim();
        s == encoding || s == "*"
      }),
      None => false,
    }
  }
}

impl<S> axum::extract::FromRequestParts<S> for AcceptEncoding
where
  S: Send + Sync,
{
  type Rejection = (StatusCode, &'static str);

  async fn from_request_parts(
    parts: &mut http::request::Parts,
    _state: &S,
  ) -> Result<Self, Self::Rejection> {
    Ok(Self(
      parts
        .headers
        .get(http::header::ACCEPT_ENCODING)
        .and_then(|v| v.to_str().ok())
        .map(String::from),
    ))
  }
}
