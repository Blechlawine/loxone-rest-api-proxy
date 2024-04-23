use axum::{
    extract::{Path, Query},
    http::{self, HeaderMap},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use axum_extra::{
    headers::{authorization::Basic, Authorization},
    typed_header::{TypedHeaderRejection, TypedHeaderRejectionReason},
    TypedHeader,
};
use serde::{Deserialize, Serialize};
use tower_http::trace::{self, TraceLayer};
use tracing::Level;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let app = Router::new().route("/*path", get(proxy)).layer(
        TraceLayer::new_for_http()
            .make_span_with(trace::DefaultMakeSpan::new().level(Level::INFO))
            .on_response(trace::DefaultOnResponse::new().level(Level::INFO)),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000")
        .await
        .unwrap();
    tracing::info!("listening on {}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

#[derive(Deserialize)]
struct ProxyQuery {
    ip: String,
}
async fn proxy(
    Path(path): Path<String>,
    Query(query): Query<ProxyQuery>,
    auth: Result<TypedHeader<Authorization<Basic>>, TypedHeaderRejection>,
) -> Response {
    let auth = match auth {
        Ok(auth) => auth,
        Err(e) => {
            if e.name() == "authorization"
                && matches!(e.reason(), TypedHeaderRejectionReason::Missing)
            {
                let mut headers = HeaderMap::new();
                headers.insert(
                    "WWW-Authenticate",
                    "Basic realm=\"loxone-rest-proxy\"".parse().unwrap(),
                );

                return (http::StatusCode::UNAUTHORIZED, headers).into_response();
            }

            return (http::StatusCode::BAD_REQUEST, e.to_string()).into_response();
        }
    };
    let url = format!("http://{}/{}", query.ip, path);
    tracing::info!("proxying to {}", url);
    let client = reqwest::Client::new();
    let result = client
        .get(url)
        .basic_auth(auth.username(), Some(auth.password()))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let parsed = quick_xml::de::from_str::<LoxoneApiXml>(&result);
    if let Ok(xml) = parsed {
        return (http::StatusCode::OK, Json(xml)).into_response();
    }

    (http::StatusCode::INTERNAL_SERVER_ERROR, "Parsing XML error").into_response()
}

#[derive(Deserialize, Serialize)]
struct LoxoneApiXml {
    #[serde(rename(deserialize = "@value"))]
    value: String,
    #[serde(rename(deserialize = "@control"))]
    control: String,
    #[serde(rename(deserialize = "@Code"))]
    code: String,
}

