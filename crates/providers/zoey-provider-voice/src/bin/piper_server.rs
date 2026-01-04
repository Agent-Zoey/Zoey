//! Piper TTS Server - Rust Implementation
//!
//! Ultra low-latency local text-to-speech server using Piper.
//! Part of the Zoey voice infrastructure.
//!
//! ## Usage
//! ```bash
//! # Start server with defaults
//! cargo run --bin piper-server --features piper-server
//!
//! # Custom configuration
//! piper-server --port 5500 --model voices/models/en_US-amy-low.onnx
//!
//! # Test
//! curl 'http://localhost:5500/api/tts?text=hello' -o test.wav
//! ```

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tower_http::cors::CorsLayer;
use tracing::{error, info, warn};

/// Piper TTS Server - Low-latency local text-to-speech
#[derive(Parser, Debug)]
#[command(name = "piper-server")]
#[command(about = "Ultra low-latency local TTS server using Piper")]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "5500")]
    port: u16,

    /// Host to bind to
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// Path to Piper binary
    #[arg(long, env = "PIPER_PATH")]
    piper: Option<PathBuf>,

    /// Path to voice model (.onnx file)
    #[arg(short, long, env = "PIPER_MODEL")]
    model: Option<PathBuf>,

    /// Speaking rate (0.5 to 2.0)
    #[arg(long, default_value = "1.0")]
    rate: f32,
}

/// Server state shared across handlers
struct AppState {
    piper_path: PathBuf,
    model_path: PathBuf,
    lib_path: PathBuf,
    rate: f32,
}

impl AppState {
    fn new(args: &Args) -> Result<Self, String> {
        // Find piper binary
        let piper_path = args.piper.clone().unwrap_or_else(|| {
            // Look in standard locations
            let candidates = [
                PathBuf::from("voices/piper/piper"),
                PathBuf::from("piper/piper"),
                PathBuf::from("/usr/local/bin/piper"),
                dirs::data_local_dir()
                    .unwrap_or_default()
                    .join("piper/piper"),
            ];
            candidates
                .into_iter()
                .find(|p| p.exists())
                .unwrap_or_else(|| PathBuf::from("voices/piper/piper"))
        });

        if !piper_path.exists() {
            return Err(format!("Piper binary not found at: {}", piper_path.display()));
        }

        // Find model
        let model_path = args.model.clone().unwrap_or_else(|| {
            let candidates = [
                PathBuf::from("voices/models/en_US-amy-low.onnx"),
                PathBuf::from("models/en_US-amy-low.onnx"),
            ];
            candidates
                .into_iter()
                .find(|p| p.exists())
                .unwrap_or_else(|| PathBuf::from("voices/models/en_US-amy-low.onnx"))
        });

        if !model_path.exists() {
            return Err(format!("Model not found at: {}", model_path.display()));
        }

        // Find library path (same directory as piper binary)
        let lib_path = piper_path.parent().unwrap_or(&piper_path).to_path_buf();

        Ok(Self {
            piper_path,
            model_path,
            lib_path,
            rate: args.rate,
        })
    }
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct TtsQuery {
    text: String,
    #[serde(default)]
    format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TtsRequest {
    text: String,
    #[serde(default)]
    voice: Option<String>,
    #[serde(default)]
    rate: Option<f32>,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    model: String,
    piper: String,
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
}

// ============================================================================
// HTTP Handlers
// ============================================================================

/// Health check endpoint
async fn health(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(HealthResponse {
        status: "ok",
        model: state
            .model_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        piper: state
            .piper_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
    })
}

/// TTS via GET with query params
async fn tts_get(
    State(state): State<Arc<AppState>>,
    Query(query): Query<TtsQuery>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    synthesize(&state, &query.text, query.format.as_deref()).await
}

/// TTS via POST with JSON body
async fn tts_post(
    State(state): State<Arc<AppState>>,
    Json(request): Json<TtsRequest>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    synthesize(&state, &request.text, None).await
}

/// TTS via POST with plain text body
async fn tts_plain(
    State(state): State<Arc<AppState>>,
    body: String,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    synthesize(&state, &body, None).await
}

/// Core synthesis function
async fn synthesize(
    state: &AppState,
    text: &str,
    format: Option<&str>,
) -> Result<Response, (StatusCode, Json<ErrorResponse>)> {
    if text.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "Empty text".to_string(),
                details: None,
            }),
        ));
    }

    // Build command
    let mut cmd = Command::new(&state.piper_path);
    cmd.arg("--model")
        .arg(&state.model_path)
        .arg("--output-raw")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set library path
    let ld_path = format!(
        "{}:{}",
        state.lib_path.display(),
        std::env::var("LD_LIBRARY_PATH").unwrap_or_default()
    );
    cmd.env("LD_LIBRARY_PATH", ld_path);

    // Spawn process
    let mut child = cmd.spawn().map_err(|e| {
        error!(error = %e, "Failed to spawn piper");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Failed to start Piper".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    // Write text to stdin
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await.map_err(|e| {
            error!(error = %e, "Failed to write to piper stdin");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "Failed to send text to Piper".to_string(),
                    details: Some(e.to_string()),
                }),
            )
        })?;
    }

    // Wait for output
    let output = child.wait_with_output().await.map_err(|e| {
        error!(error = %e, "Piper process failed");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Piper process failed".to_string(),
                details: Some(e.to_string()),
            }),
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(stderr = %stderr, "Piper returned error");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "Synthesis failed".to_string(),
                details: Some(stderr.to_string()),
            }),
        ));
    }

    // Convert raw PCM to WAV
    let pcm_data = output.stdout;
    let wav_data = pcm_to_wav(&pcm_data, 22050);

    // Return audio
    let content_type = match format {
        Some("raw") | Some("pcm") => "audio/pcm",
        _ => "audio/wav",
    };

    let body = if content_type == "audio/pcm" {
        pcm_data
    } else {
        wav_data
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CONTENT_LENGTH, body.len())
        .body(axum::body::Body::from(body))
        .unwrap())
}

/// Convert raw PCM to WAV format
fn pcm_to_wav(pcm: &[u8], sample_rate: u32) -> Vec<u8> {
    let channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate = sample_rate * channels as u32 * bits_per_sample as u32 / 8;
    let block_align = channels * bits_per_sample / 8;
    let data_size = pcm.len() as u32;

    let mut wav = Vec::with_capacity(44 + pcm.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm);

    wav
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("piper_server=info".parse().unwrap()),
        )
        .init();

    // Parse args
    let args = Args::parse();

    // Initialize state
    let state = match AppState::new(&args) {
        Ok(s) => Arc::new(s),
        Err(e) => {
            eprintln!("╔══════════════════════════════════════════════════════════════╗");
            eprintln!("║                    ERROR                                     ║");
            eprintln!("╠══════════════════════════════════════════════════════════════╣");
            eprintln!("║  {}", e);
            eprintln!("║");
            eprintln!("║  Make sure you have:");
            eprintln!("║    1. Downloaded Piper: ./scripts/setup-piper-native.sh");
            eprintln!("║    2. Set PIPER_PATH environment variable");
            eprintln!("║    3. Or use --piper and --model flags");
            eprintln!("╚══════════════════════════════════════════════════════════════╝");
            std::process::exit(1);
        }
    };

    // Build router
    let app = Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/api/tts", get(tts_get).post(tts_post))
        .route("/synthesize", post(tts_plain))
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    // Bind address
    let addr: SocketAddr = format!("{}:{}", args.host, args.port)
        .parse()
        .expect("Invalid address");

    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║              PIPER TTS SERVER (Rust)                         ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Address: http://{}:{}", args.host, args.port);
    println!(
        "║  Model:   {}",
        state
            .model_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    println!("║  Piper:   {}", state.piper_path.display());
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Endpoints:");
    println!("║    GET  /                    Health check");
    println!("║    GET  /api/tts?text=hello  Synthesize (query param)");
    println!("║    POST /api/tts             Synthesize (JSON body)");
    println!("║    POST /synthesize          Synthesize (plain text)");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║  Test:");
    println!(
        "║    curl 'http://localhost:{}/api/tts?text=hello' -o test.wav",
        args.port
    );
    println!("╚══════════════════════════════════════════════════════════════╝");

    info!(
        addr = %addr,
        model = %state.model_path.display(),
        "Piper TTS server starting"
    );

    // Start server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

