use std::{io, net::SocketAddr, path::{Path as StdPath, PathBuf}, time::Duration};
use std::collections::HashSet;

use axum::{
    Json, Router,
    body::Body,
    extract::{Path, Query, State},
    http::{StatusCode, header, HeaderMap},
    response::{IntoResponse, Response},
    routing::get,
};
use dotenvy::dotenv;
use moka::future::Cache;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::process::Command;
use tokio::fs;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio_util::io::ReaderStream;
use tower_http::{compression::CompressionLayer, cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt};
use futures_util::StreamExt; // <-- Adicione esta linha!
// Linha opcional, mas recomendada para a versão melhorada:
use tokio::io::{AsyncSeekExt, SeekFrom};



#[derive(Clone)]
struct AppState {
    http: Client,
    api_key: String,      // OMDb API key
    cache: Cache<String, serde_json::Value>,
    tmdb_key: String,     // <-- add TMDB key
}

#[derive(Debug, Deserialize)]
struct TmdbList {
    results: Vec<TmdbMovie>,
}

#[derive(Debug, Deserialize)]
struct TmdbMovie {
    id: u64,
    title: Option<String>,
    name: Option<String>, // fallback for TV shows
}

#[derive(Debug, Serialize)]
struct CombinedMovie {
    tmdb_title: String,
    omdb: serde_json::Value,
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
    let tmdb_key = std::env::var("TMDB_API_KEY").expect("Defina TMDB_API_KEY no ambiente (.env)");

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
        tmdb_key,
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
        // .route("/stream", axum::routing::get(download_and_stream))
        .route("/stream", axum::routing::get(download_and_stream))
        .route("/movies/trending", get(movies_trending))
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

async fn find_downloaded_file(base_dir: &StdPath, filename: &str) -> Option<PathBuf> {
    let mut entries = match fs::read_dir(base_dir).await {
        Ok(rd) => rd,
        Err(_) => return None,
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == filename).unwrap_or(false) {
            return Some(path);
        } else if path.is_dir() {
            // Aqui criamos uma future "boxed" para a chamada recursiva
            if let Some(found) = Box::pin(find_downloaded_file(&path, filename)).await {
                return Some(found);
            }
        }
    }

    None
}
async fn download_and_stream(Query(params): Query<TorrentParams>, headers: HeaderMap) -> Result<Response, (StatusCode, String)>  {
    let download_dir = PathBuf::from("./downloads");
    tokio::fs::create_dir_all(&download_dir).await.unwrap();

    let filepath = match find_downloaded_file(&download_dir, &params.filename).await {
        Some(p) => p,
        None => {
            println!("File not found, starting aria2c download...");

            let magnet_link = if params.magnet.starts_with("magnet:?") {
                params.magnet.clone()
            } else {
                format!("magnet:?xt=urn:btih:{}", params.magnet)
            };

            let status = Command::new("aria2c")
                .arg("--dir")
                .arg(&download_dir)
                .arg("--out")
                .arg(&params.filename)
                .arg("--seed-time=0")
                .arg(magnet_link)
                .arg("--enable-dht=true")
                .arg("--enable-peer-exchange=true")
                .arg("--bt-tracker=udp://tracker.opentrackr.org:1337/announce,udp://open.stealth.si:80/announce,udp://tracker.cyberia.is:6969/announce")
                .status()
                .await;


            println!("aria2c finished: {:?}", status);

            if !matches!(status, Ok(s) if s.success()) {
                return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Download failed")));
            }

            find_downloaded_file(&download_dir, &params.filename).await
                .ok_or_else(|| (StatusCode::INTERNAL_SERVER_ERROR, "File not found after download".to_string()))?
            
        }
    };

    println!("Checking file at {:?}", filepath);

    // Stream the file
    if !filepath.exists() {
        return Err((StatusCode::NOT_FOUND, "Video not found".to_string()));
    }

    let mut file = match File::open(&filepath).await {
        Ok(file) => file,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to open video file: {}", err))),
    };

    let meta = match file.metadata().await {
        Ok(meta) => meta,
        Err(err) => return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to get video metadata: {}", err))),
    };
    let file_size = meta.len();

    let range = headers
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(|s| s.strip_prefix("bytes="));

    if let Some(range) = range {
        let (start, end) = parse_range(range, file_size).unwrap_or((0, file_size - 1));
        let chunk_size = (end - start) + 1;

        // Mover o cursor do arquivo para o 'start' do range
        if let Err(err) = file.seek(SeekFrom::Start(start)).await {
            return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to seek file: {}", err)));
        }

        // Criar um stream que lê apenas o 'chunk_size' necessário
        let stream = ReaderStream::new(file).take(chunk_size as usize);

        let body = Body::from_stream(stream);

        let mut response_headers = HeaderMap::new();
        response_headers.insert(
            header::CONTENT_RANGE,
            format!("bytes {}-{}/{}", start, end, file_size).parse().unwrap(),
        );
        response_headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());
        response_headers.insert(header::CONTENT_LENGTH, chunk_size.to_string().parse().unwrap());
        response_headers.insert(header::CONTENT_TYPE, "video/mp4".parse().unwrap());

        return Ok((StatusCode::PARTIAL_CONTENT, response_headers, body).into_response());
    }

    // Se não houver 'Range', transmite o arquivo inteiro
    let stream = ReaderStream::new(file);
    let body = Body::from_stream(stream);

    let mut response_headers = HeaderMap::new();
    response_headers.insert(header::CONTENT_TYPE, "video/mp4".parse().unwrap());
    response_headers.insert(header::CONTENT_LENGTH, file_size.to_string().parse().unwrap());
    response_headers.insert(header::ACCEPT_RANGES, "bytes".parse().unwrap());

    Ok((StatusCode::OK, response_headers, body).into_response())
}

fn parse_range(range_str: &str, file_size: u64) -> Option<(u64, u64)> {
    let mut parts = range_str.split('-');
    let start = parts.next()?.parse::<u64>().ok()?;
    let end = match parts.next() {
        Some("") | None => file_size - 1,
        Some(end_str) => end_str.parse::<u64>().ok()?,
    };

    if start > end || end >= file_size {
        return None;
    }

    Some((start, end))
}

#[derive(Serialize)]
struct OmdbMovieShort {
    Poster: String,
    Title: String,
    Type: String,
    Year: String,
    imdbID: String,
}

async fn movies_trending(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let key = "movies:trending".to_string();
    if let Some(cached) = state.cache.get(&key).await {
        return Ok(Json(cached));
    }

    let client = &state.http;

    // Get trending
    let trending_url = format!(
        "https://api.themoviedb.org/3/trending/movie/week?api_key={}",
        state.tmdb_key
    );
    let trending: TmdbList = client
        .get(&trending_url)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    // Get now playing
    let releases_url = format!(
        "https://api.themoviedb.org/3/movie/now_playing?api_key={}&language=en-US&page=1",
        state.tmdb_key
    );
    let releases: TmdbList = client
        .get(&releases_url)
        .send()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?
        .json()
        .await
        .map_err(|e| ApiError::Upstream(e.to_string()))?;

    // Merge lists
    let all = trending.results.into_iter().chain(releases.results);

    let mut seen_ids = HashSet::new();
    let mut combined: Vec<OmdbMovieShort> = Vec::new();

    for m in all {
        let title = m.title.or(m.name).unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let omdb_url = format!(
            "https://www.omdbapi.com/?apikey={}&t={}&r=json",
            state.api_key,
            urlencoding::encode(&title)
        );

        if let Ok(resp) = client.get(&omdb_url).send().await {
            if resp.status().is_success() {
                if let Ok(omdb_data) = resp.json::<serde_json::Value>().await {
                    if omdb_data.get("Response") != Some(&serde_json::Value::String("False".into())) {
                        if let Some(imdb_id) = omdb_data.get("imdbID").and_then(|v| v.as_str()) {
                            if seen_ids.contains(imdb_id) {
                                continue; // skip duplicates
                            }
                            seen_ids.insert(imdb_id.to_string());

                            combined.push(OmdbMovieShort {
                                Poster: omdb_data.get("Poster").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                Title: omdb_data.get("Title").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                Type: omdb_data.get("Type").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                Year: omdb_data.get("Year").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
                                imdbID: imdb_id.to_string(),
                            });
                        }
                    }
                }
            }
        }
    }

    let json = serde_json::json!({
        "results": combined,
        "total": combined.len().to_string(),
        "type": "movie"
    });

    state.cache.insert(key, json.clone()).await;
    Ok(Json(json))
}
