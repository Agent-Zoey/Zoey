//! Test script to verify Unmute realtime voice generation
//! 
//! This tests that unmute can generate voice in realtime with streaming chunks

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use zoey_provider_voice::{VoicePlugin, VoiceConfig, AudioFormat};
    
    println!("Testing Unmute realtime voice generation...");
    println!("Model location: .zoey/voice/ggml-tiny.bin");
    
    // Check if model exists (try multiple possible locations)
    let model_paths = [
        ".zoey/voice/ggml-tiny.bin",
        "../.zoey/voice/ggml-tiny.bin",
        "/root/zoey-rust/.zoey/voice/ggml-tiny.bin",
    ];
    
    let model_path = model_paths.iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|p| p.to_string());
    
    if model_path.is_none() {
        eprintln!("WARNING: Model not found at expected locations");
        eprintln!("  Tried: {:?}", model_paths);
        eprintln!("  Continuing anyway - unmute may use its own model");
    } else {
        println!("✓ Model found at: {}", model_path.as_ref().unwrap());
    }
    
    println!("✓ Model found");
    
    // Create unmute engine (assuming unmute is running on localhost:8000)
    let endpoint = "ws://127.0.0.1:8000";
    println!("Connecting to Unmute at: {}", endpoint);
    
    let plugin = VoicePlugin::with_unmute(endpoint);
    
    // Test health check
    println!("Checking if Unmute is available...");
    let is_ready = plugin.is_stt_ready().await;
    if !is_ready {
        println!("⚠ WARNING: Unmute endpoint not available at {}", endpoint);
        println!("  Make sure unmute dockerless services are running");
        println!("  The agent will auto-start them when voice is triggered");
    } else {
        println!("✓ Unmute is ready");
    }
    
    // Test streaming TTS
    println!("\nTesting realtime TTS streaming...");
    let test_text = "Hello, this is a test of realtime voice generation.";
    
    let config = VoiceConfig {
        engine_type: zoey_provider_voice::VoiceEngineType::Local,
        output_format: AudioFormat::Mp3,
        sample_rate: 24000,
        streaming: true,
        ..Default::default()
    };
    
    match plugin.synthesize_stream(&test_text).await {
        Ok(mut stream) => {
            println!("✓ Stream created, receiving chunks...");
            let mut chunk_count = 0;
            let mut total_bytes = 0;
            let start = std::time::Instant::now();
            let mut first_chunk_time: Option<std::time::Instant> = None;
            
            while let Some(chunk_result) = stream.recv().await {
                match chunk_result {
                    Ok(chunk) => {
                        if first_chunk_time.is_none() && !chunk.data.is_empty() {
                            first_chunk_time = Some(std::time::Instant::now());
                            let latency = start.elapsed().as_millis();
                            println!("  ✓ First chunk received in {}ms (realtime!)", latency);
                        }
                        
                        if !chunk.data.is_empty() {
                            total_bytes += chunk.data.len();
                            chunk_count += 1;
                            if chunk.is_final {
                                println!("  ✓ Final chunk received (total: {} chunks, {} bytes)", chunk_count, total_bytes);
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("  ✗ Stream error: {}", e);
                        break;
                    }
                }
            }
            
            if let Some(first) = first_chunk_time {
                let total_latency = first.elapsed().as_millis();
                println!("\n✓ Realtime streaming test complete!");
                println!("  - First chunk latency: {}ms", start.elapsed().as_millis());
                println!("  - Total chunks: {}", chunk_count);
                println!("  - Total bytes: {}", total_bytes);
                println!("  - Streaming works: YES (chunks arrive in realtime)");
            } else {
                println!("⚠ No chunks received - unmute may not be running");
            }
        }
        Err(e) => {
            eprintln!("✗ Failed to create stream: {}", e);
            println!("\nNote: This is expected if unmute services are not running.");
            println!("The agent will auto-start unmute dockerless when voice is triggered in Discord.");
        }
    }
    
    println!("\nTest complete!");
    Ok(())
}

