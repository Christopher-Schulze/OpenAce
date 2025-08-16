use openace_rust::engines::{SegmenterEngine, StreamerEngine};
use std::path::PathBuf;
use tokio::sync::mpsc;
use tokio::test;

#[tokio::test]
async fn test_segmenter_to_streamer_interaction() {
    // Setup SegmenterEngine
    let segmenter = SegmenterEngine::new(Default::default()).await.unwrap();
    let input_path = PathBuf::from("tests/fixtures/test_media/sample.mp4");
    let output_dir = PathBuf::from("tests/fixtures/output_segments");
    let segment_duration = 10;
    let format = "mp4".to_string();
    let quality = "high".to_string();

    // Perform segmentation
    let segments = segmenter.perform_segmentation(input_path, output_dir, segment_duration, format, quality).await.unwrap();

    // Setup StreamerEngine
    let (tx, _rx) = mpsc::channel(32);
    let streamer = StreamerEngine::new(tx).await.unwrap();

    // Simulate adding segments to streamer
    for segment in segments {
        streamer.add_segment(segment).await.unwrap();
    }

    // Verify streamer has segments
    assert!(!streamer.get_available_streams().await.is_empty());
}

#[tokio::test]
async fn test_transport_integration() {
    // Setup TransportEngine
    let transport = TransportEngine::new(Default::default()).await.unwrap();
    let peer_id = "test_peer".to_string();
    let message = AceMessage::new("hello".into());

    // Create connection
    transport.create_connection(peer_id.clone()).await.unwrap();

    // Send message
    transport.send_message(peer_id, message).await.unwrap();

    // Verify connection exists
    assert!(transport.has_connection(&peer_id).await);
}

#[tokio::test]
async fn test_concurrent_engine_operations() {
    let segmenter = SegmenterEngine::new(Default::default()).await.unwrap();
    let streamer = StreamerEngine::new(mpsc::channel(32).0).await.unwrap();
    let transport = TransportEngine::new(Default::default()).await.unwrap();

    // Concurrent tasks
    let seg_handle = tokio::spawn(async move {
        let input = PathBuf::from("tests/fixtures/test_media/sample.mp4");
        let output = PathBuf::from("tests/fixtures/output_segments");
        segmenter.perform_segmentation(input, output, 10, "mp4".to_string(), "high".to_string()).await.unwrap();
    });

    let stream_handle = tokio::spawn(async move {
        streamer.add_segment("test_segment".into()).await.unwrap();
    });

    let trans_handle = tokio::spawn(async move {
        transport.create_connection("test_peer".to_string()).await.unwrap();
    });

    // Await all
    seg_handle.await.unwrap();
    stream_handle.await.unwrap();
    trans_handle.await.unwrap();
}