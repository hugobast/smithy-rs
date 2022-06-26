/*
 * Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
 * SPDX-License-Identifier: Apache-2.0.
 */

// This code was heavily inspired by https://github.com/hanabu/lambda-web

use futures_util::future::BoxFuture;
use http::{uri, Response};
use http_body::Body as HttpBody;
use hyper::Body as HyperBody;
#[allow(unused_imports)]
use lambda_http::{Body as LambdaBody, Request, RequestExt as _};
use std::{
    convert::Infallible,
    fmt::Debug,
    task::{Context, Poll},
};
use tower::Service;

type HyperRequest = http::Request<HyperBody>;

/// A [`MakeService`] that produces AWS Lambda compliant services.
///
/// [`MakeService`]: tower::make::MakeService
#[derive(Debug, Clone)]
pub struct IntoMakeLambdaService<S> {
    service: S,
}

impl<S> IntoMakeLambdaService<S> {
    pub(super) fn new(service: S) -> Self {
        Self { service }
    }
}

impl<S, B> Service<Request> for IntoMakeLambdaService<S>
where
    S: Service<HyperRequest, Response = Response<B>, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
    B: HttpBody + Send + 'static,
{
    type Response = Response<B>;
    type Error = Infallible;
    type Future = MakeRouteLambdaServiceFuture<B>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.service.poll_ready(cx).map_err(|err| err.into())
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let clone = self.service.clone();
        let mut inner = std::mem::replace(&mut self.service, clone);

        let fut = async move {
            let hyper_request = lambda_to_hyper_request(req)?;
            inner.call(hyper_request).await
        };

        MakeRouteLambdaServiceFuture::new(Box::pin(fut))
    }
}

opaque_future! {
    /// Response future for [`IntoMakeLambdaService`] services.
    pub type MakeRouteLambdaServiceFuture<B> = BoxFuture<'static, Result<Response<B>, Infallible>>;
}

fn lambda_to_hyper_request(request: Request) -> Result<HyperRequest, Infallible> {
    tracing::debug!("Converting Lambda to Hyper request...");
    // Raw HTTP path without any stage information
    let raw_path = request.raw_http_path();
    let (mut parts, body) = request.into_parts();
    let mut path = String::from(parts.uri.path());
    if !raw_path.is_empty() && raw_path != path {
        tracing::debug!("Recreating URI from raw HTTP path.");
        path = raw_path;
        let uri_parts: uri::Parts = parts.uri.into();
        let path_and_query = uri_parts.path_and_query.unwrap();
        if let Some(query) = path_and_query.query() {
            path.push('?');
            path.push_str(query);
        }
        parts.uri = uri::Uri::builder()
            .authority(uri_parts.authority.unwrap())
            .scheme(uri_parts.scheme.unwrap())
            .path_and_query(path)
            .build()
            .unwrap();
    }
    let body = match body {
        LambdaBody::Empty => HyperBody::empty(),
        LambdaBody::Text(s) => HyperBody::from(s),
        LambdaBody::Binary(v) => HyperBody::from(v),
    };
    // We need to maintain all parts including the extensions
    let req = http::Request::from_parts(parts, body);
    tracing::debug!("Hyper request converted successfully.");
    Ok(req)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{body::to_boxed, test_helpers::get_body_as_string};
    use http::header::CONTENT_TYPE;
    use hyper::Response;
    use lambda_http::request::LambdaRequest;
    use serde_json::error::Error as JsonError;

    fn from_str(s: &str) -> Result<Request, JsonError> {
        serde_json::from_str(s).map(LambdaRequest::into)
    }

    #[test]
    fn traits() {
        use crate::test_helpers::*;

        assert_send::<IntoMakeLambdaService<()>>();
        assert_sync::<IntoMakeLambdaService<()>>();
    }

    #[test]
    fn converts_apigw_event_to_hyper_request() {
        let input = include_str!("../../test_data/apigw_request.json");
        let req = from_str(input).expect("failed to parse request");
        let result = lambda_to_hyper_request(req).expect("failed to convert to hyper request");
        assert_eq!(result.method(), "GET");
        assert_eq!(
            result.uri(),
            "https://wt6mne2s9k.execute-api.us-west-2.amazonaws.com/hello?name=me"
        );
        // assert_eq!(result.raw_http_path(), "/hello");
    }

    #[tokio::test]
    async fn converts_apigw_v2_event_to_hyper_request() {
        let input = include_str!("../../test_data/apigw_v2_request.json");
        let req = from_str(input).expect("failed to parse request");
        let mut result = lambda_to_hyper_request(req).expect("failed to convert to hyper request");
        assert_eq!(result.method(), "POST");
        assert_eq!(result.uri(), "https://id.execute-api.us-east-1.amazonaws.com/my/path?parameter1=value1&parameter1=value2&parameter2=value");
        assert_eq!(
            result
                .headers()
                .get(CONTENT_TYPE)
                .map(|h| h.to_str().expect("invalid header")),
            Some("application/json")
        );
        let actual_body = get_body_as_string(result.body_mut()).await;
        assert_eq!(actual_body, r##"{"message":"Hello from Lambda"}"##);
    }

    #[tokio::test]
    async fn converts_hyper_to_lambda_response() {
        let builder = Response::builder()
            .status(200)
            .header(CONTENT_TYPE, "application/json")
            .body(to_boxed("{\"hello\":\"lambda\"}"));
        let res = builder.expect("failed to parse response");
        let mut result = hyper_to_lambda_response(res)
            .await
            .expect("failed to convert to lambda request");
        assert_eq!(result.status(), 200);
        assert_eq!(
            result
                .headers()
                .get(CONTENT_TYPE)
                .map(|h| h.to_str().expect("invalid header")),
            Some("application/json")
        );
        let actual_body = get_body_as_string(result.body_mut()).await;
        assert_eq!(actual_body, r##"{"hello":"lambda"}"##);
    }
}
