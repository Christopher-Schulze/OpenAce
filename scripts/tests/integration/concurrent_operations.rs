use openace_rust::engines::{SegmenterEngine, StreamerEngine};
use std::path::PathBuf;
use tokio::test;
use tokio::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_segmentation() {
    let engine = Arc::new(SegmenterEngine::new(Default::default()).await.unwrap());
    let mut handles = vec![];

    for i in 0..10 {
        let engine_clone = engine.clone();
        let input_path = PathBuf::from(format!("tests/fixtures/test_media/sample{}.mp4", i));
        let output_dir = PathBuf::from("tests/fixtures/output_segments");
        handles.push(tokio::spawn(async move {
            engine_clone.perform_segmentation(input_path, output_dir, 10, "mp4".to_string(), "high".to_string()).await
        }));
    }

    for handle in handles {
        handle.await.unwrap().unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_streaming() {
    let (tx, _rx) = tokio::sync::mpsc::channel(32);
    let streamer = Arc::new(StreamerEngine::new(tx).await.unwrap());
    let mut handles = vec![];

    for i in 0..100 {
        let streamer_clone = streamer.clone();
        handles.push(tokio::spawn(async move {
            // Simulate client connection and streaming
            let stream_id = format!("stream{}", i);
            streamer_clone.get_stream(stream_id).await
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }
}