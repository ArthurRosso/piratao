use std::{io, net::SocketAddr, path::PathBuf, process::Command, time::Duration};

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{Response, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use dotenvy::dotenv;
use moka::future::Cache;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};

#[derive(Clone)]
struct AppState {
    http: Client,
    api_key: String,
    cache: Cache<String, serde_json::Value>,
}

#[derive(Debug, Error)]
enum ApiError {
    #[error("Upstream error: {0}")]
    Upstream(String),
    #[error("Bad request: {0}")]
    BadRequest(String),
    #[error("Internal error")]
    Internal,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (code, msg) = match self {
            ApiError::Upstream(m) => (StatusCode::BAD_GATEWAY, m),
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into()),
        };
        (code, Json(serde_json::json!({"error": msg}))).into_response()
    }
}

#[derive(Debug, Deserialize)]
struct SearchParams {
    q: String,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_type")]
    r#type: String,
}
fn default_page() -> u32 {
    1
}
fn default_type() -> String {
    "movie".to_string()
}

#[derive(Debug, Serialize, Deserialize)]
struct OmdbSearchItem {
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Year")]
    year: String,
    #[serde(rename = "imdbID")]
    imdb_id: String,
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Poster")]
    poster: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct OmdbSearchResp {
    #[serde(rename = "Search")]
    search: Option<Vec<OmdbSearchItem>>,
    #[serde(rename = "totalResults")]
    total: Option<String>,
    #[serde(rename = "Response")]
    ok: String,
    #[serde(rename = "Error")]
    error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OmdbMovieDetail {
    #[serde(rename = "Title")]
    title: String,
    #[serde(rename = "Year")]
    year: String,
    #[serde(rename = "imdbID")]
    imdb_id: String,
    #[serde(rename = "Type")]
    kind: String,
    #[serde(rename = "Genre")]
    genre: String,
    #[serde(rename = "Director")]
    director: String,
    #[serde(rename = "Actors")]
    actors: String,
    #[serde(rename = "Plot")]
    plot: String,
    #[serde(rename = "Poster")]
    poster: String,
    #[serde(rename = "imdbRating")]
    imdb_rating: String,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    dotenv().ok();

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    fmt().with_env_filter(filter).init();

    let api_key = std::env::var("OMDB_API_KEY").expect("Defina OMDB_API_KEY no ambiente (.env)");

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    // Cliente HTTP com pooling, gzip/brotli, timeout e retry simples (manual ao chamar)
    let http = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(8))
        .pool_max_idle_per_host(8)
        .build()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

    // Cache TTL curto para reduzir latência e chamadas externas
    let cache: Cache<String, serde_json::Value> = Cache::builder()
        .time_to_live(Duration::from_secs(60))
        .max_capacity(10_000)
        .build();

    let state = AppState {
        http,
        api_key,
        cache,
    };

    // let app = Router::new()
    //     .route("/health", get(health))
    //     .route("/search", get(search_movies))
    //     .route("/movie/:imdb_id", get(movie_detail))
    //     .with_state(state)
    //     // Apply each layer individually
    //     .layer(CompressionLayer::new())
    //     //.layer(TimeoutLayer::new(Duration::from_secs(10)))
    //     .layer(TraceLayer::new_for_http())
    //     .layer(CorsLayer::permissive());
    let app = Router::new()
        .route("/health", get(health))
        .route("/search", get(search_movies))
        .route("/movie/:imdb_id", get(movie_detail))
        .route("/torrentio/movie/:imdb_id", get(torrentio_movie))
        .route(
            "/torrentio/show/:imdb_id/:season/:episode",
            get(torrentio_episode),
        )
        .route("/stream-torrent", axum::routing::get(download_and_stream))
        .with_state(state)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = ([0, 0, 0, 0], port).into();
    let listener = TcpListener::bind(addr).await?;
    info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn search_movies(
    State(state): State<AppState>,
    Query(params): Query<SearchParams>,
) -> Result<impl IntoResponse, ApiError> {
    if params.q.trim().is_empty() {
        return Err(ApiError::BadRequest("q vazio".into()));
    }

    let key = format!(
        "search:q={}:page={}:type={}",
        params.q, params.page, params.r#type
    );

    if let Some(cached) = state.cache.get(&key).await {
        return Ok(Json(cached));
    }

    let url = format!(
        "https://www.omdbapi.com/?apikey={}&s={}&page={}&type={}&r=json",
        state.api_key,
        urlencoding::encode(&params.q),
        params.page,
        urlencoding::encode(&params.r#type),
    );

    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ApiError::Upstream(format!("status {}", resp.status())));
    }

    let body: OmdbSearchResp = resp
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if body.ok != "True" {
        let msg = body.error.unwrap_or_else(|| "unknown".into());
        return Err(ApiError::Upstream(msg));
    }

    let json = serde_json::json!({
        "query": params.q,
        "page": params.page,
        "type": params.r#type,
        "total": body.total,
        "results": body.search.unwrap_or_default(),
    });

    state.cache.insert(key, json.clone()).await;
    Ok(Json(json))
}

async fn movie_detail(
    State(state): State<AppState>,
    Path(imdb_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if imdb_id.trim().is_empty() {
        return Err(ApiError::BadRequest("imdb_id vazio".into()));
    }

    let key = format!("detail:{}", imdb_id);
    if let Some(cached) = state.cache.get(&key).await {
        return Ok(Json(cached));
    }

    let url = format!(
        "https://www.omdbapi.com/?apikey={}&i={}&plot=full&r=json",
        state.api_key,
        urlencoding::encode(&imdb_id),
    );

    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ApiError::Upstream(format!("status {}", resp.status())));
    }

    // Não mapeamos tudo: retornamos JSON cru para flexibilidade
    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if body.get("Response") == Some(&serde_json::Value::String("False".into())) {
        let msg = body
            .get("Error")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        return Err(ApiError::Upstream(msg.into()));
    }

    state.cache.insert(key, body.clone()).await;
    Ok(Json(body))
}

async fn torrentio_movie(
    State(state): State<AppState>,
    Path(imdb_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    if imdb_id.trim().is_empty() {
        return Err(ApiError::BadRequest("imdb_id vazio".into()));
    }

    let key = format!("torrentio:movie:{}", imdb_id);
    if let Some(cached) = state.cache.get(&key).await {
        return Ok(Json(cached));
    }

    let url = format!("https://torrentio.strem.fun/stream/movie/{}.json", imdb_id);

    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ApiError::Upstream(format!("status {}", resp.status())));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    state.cache.insert(key, body.clone()).await;
    Ok(Json(body))
}

async fn torrentio_episode(
    State(state): State<AppState>,
    Path((imdb_id, season, episode)): Path<(String, String, String)>,
) -> Result<impl IntoResponse, ApiError> {
    if imdb_id.trim().is_empty() {
        return Err(ApiError::BadRequest("imdb_id vazio".into()));
    }

    let key = format!("torrentio:show:{}:S{}E{}", imdb_id, season, episode);
    if let Some(cached) = state.cache.get(&key).await {
        return Ok(Json(cached));
    }

    let url = format!(
        "https://torrentio.strem.fun/stream/series/{}/{}-{}/.json",
        imdb_id, season, episode
    );

    let resp = state
        .http
        .get(&url)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    if !resp.status().is_success() {
        return Err(ApiError::Upstream(format!("status {}", resp.status())));
    }

    let body: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    state.cache.insert(key, body.clone()).await;
    Ok(Json(body))
}

#[derive(Deserialize)]
struct TorrentParams {
    magnet: String,
    filename: String, // nome do arquivo a ser servido
}

async fn download_and_stream(Query(params): Query<TorrentParams>) -> impl IntoResponse {
    let download_dir = PathBuf::from("./downloads");
    tokio::fs::create_dir_all(&download_dir).await.unwrap();

    let filepath = download_dir.join(&params.filename);

    // If the file doesn't exist, download it
    if !filepath.exists() {
        let status = Command::new("aria2c")
            .arg("--dir")
            .arg(&download_dir)
            .arg("--out")
            .arg(&params.filename)
            .arg("--seed-time=0")
            .arg(&params.magnet)
            .status(); // <-- ERROR 1: Missing .await here

        match status {
            Ok(s) if s.success() => (),
            _ => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Download failed").into_response();
            }
        }
    }

    // Stream the file
    let file = match File::open(&filepath).await {
        Ok(f) => f,
        Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
    };

    let stream = ReaderStream::new(file);

    // Create a body from the stream for Axum
    let body = Body::from_stream(stream);

    Response::builder()
        .header(header::CONTENT_TYPE, "video/mp4") // Note: You might want to determine this dynamically
        .header(header::ACCEPT_RANGES, "bytes")
        .body(body)
        .unwrap()
        .into_response()
}
