//! Streaming response support

use crate::{ZoeyError, Result};
use tokio::sync::mpsc;

/// Streaming text chunk
#[derive(Debug, Clone)]
pub struct TextChunk {
    /// Chunk text
    pub text: String,
    /// Whether this is the final chunk
    pub is_final: bool,
    /// Optional metadata
    pub metadata: Option<serde_json::Value>,
}

/// Stream of text chunks
pub type TextStream = mpsc::Receiver<Result<TextChunk>>;

/// Stream sender
pub type TextStreamSender = mpsc::Sender<Result<TextChunk>>;

/// Create a new text stream
pub fn create_text_stream(buffer_size: usize) -> (TextStreamSender, TextStream) {
    mpsc::channel(buffer_size)
}

/// Streaming response handler
pub struct StreamHandler {
    sender: TextStreamSender,
}

impl StreamHandler {
    /// Create a new stream handler
    pub fn new(sender: TextStreamSender) -> Self {
        Self { sender }
    }

    /// Send a chunk of text
    pub async fn send_chunk(&self, text: String, is_final: bool) -> Result<()> {
        self.sender
            .send(Ok(TextChunk {
                text,
                is_final,
                metadata: None,
            }))
            .await
            .map_err(|e| ZoeyError::other(format!("Failed to send chunk: {}", e)))
    }

    /// Send a chunk of text with metadata
    pub async fn send_chunk_with_meta(&self, text: String, is_final: bool, metadata: Option<serde_json::Value>) -> Result<()> {
        self.sender
            .send(Ok(TextChunk {
                text,
                is_final,
                metadata,
            }))
            .await
            .map_err(|e| ZoeyError::other(format!("Failed to send chunk: {}", e)))
    }

    /// Send an error
    pub async fn send_error(&self, error: ZoeyError) -> Result<()> {
        self.sender
            .send(Err(error))
            .await
            .map_err(|e| ZoeyError::other(format!("Failed to send error: {}", e)))
    }

    /// Send final chunk and close stream
    pub async fn finish(&self, text: String) -> Result<()> {
        self.send_chunk(text, true).await
    }
}

/// Collect all chunks from a stream into a single string
pub async fn collect_stream(mut stream: TextStream) -> Result<String> {
    let mut result = String::new();

    while let Some(chunk_result) = stream.recv().await {
        let chunk = chunk_result?;
        result.push_str(&chunk.text);

        if chunk.is_final {
            break;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stream_creation() {
        let (sender, mut receiver) = create_text_stream(10);

        // Send chunks
        sender
            .send(Ok(TextChunk {
                text: "Hello".to_string(),
                is_final: false,
                metadata: None,
            }))
            .await
            .unwrap();

        sender
            .send(Ok(TextChunk {
                text: " World".to_string(),
                is_final: true,
                metadata: None,
            }))
            .await
            .unwrap();

        // Receive chunks
        let chunk1 = receiver.recv().await.unwrap().unwrap();
        assert_eq!(chunk1.text, "Hello");
        assert!(!chunk1.is_final);

        let chunk2 = receiver.recv().await.unwrap().unwrap();
        assert_eq!(chunk2.text, " World");
        assert!(chunk2.is_final);
    }

    #[tokio::test]
    async fn test_stream_handler() {
        let (sender, receiver) = create_text_stream(10);
        let handler = StreamHandler::new(sender);

        // Send chunks
        tokio::spawn(async move {
            handler
                .send_chunk("Chunk 1".to_string(), false)
                .await
                .unwrap();
            handler
                .send_chunk("Chunk 2".to_string(), false)
                .await
                .unwrap();
            handler.finish("Final chunk".to_string()).await.unwrap();
        });

        // Collect stream
        let result = collect_stream(receiver).await.unwrap();
        assert_eq!(result, "Chunk 1Chunk 2Final chunk");
    }

    #[tokio::test]
    async fn test_stream_error() {
        let (sender, mut receiver) = create_text_stream(10);

        sender
            .send(Err(ZoeyError::other("Test error")))
            .await
            .unwrap();

        let chunk_result = receiver.recv().await.unwrap();
        assert!(chunk_result.is_err());
    }
}
