use {axum::http, http::StatusCode};

pub(crate) struct AcceptJson(pub(crate) bool);

impl<S> axum::extract::FromRequestParts<S> for AcceptJson
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
        .get("accept")
        .map(|value| value == "application/json")
        .unwrap_or_default(),
    ))
  }
}
