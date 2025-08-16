use openace_rust::engines::{MainEngine, SegmenterEngine, StreamerEngine};
use std::path::PathBuf;
use tokio::test;

#[tokio::test]
async fn test_full_workflow() {
    // Setup MainEngine
    let main_engine = MainEngine::new(Default::default()).await.unwrap();
    main_engine.start().await.unwrap();

    // Simulate segmentation
    let segmenter = main_engine.get_segmenter().unwrap();
    let input_path = PathBuf::from("tests/fixtures/test_media/sample.mp4");
    let output_dir = PathBuf::from("tests/fixtures/output_segments");
    let segments = segmenter.perform_segmentation(input_path, output_dir, 10, "mp4".to_string(), "high".to_string()).await.unwrap();

    // Add to streamer
    let streamer = main_engine.get_streamer().unwrap();
    for segment in segments {
        streamer.add_segment(segment).await.unwrap();
    }

    // Simulate client streaming with mock client
    let mock_client = MockClient::new();
    mock_client.connect_to_streamer(&streamer).await.unwrap();
    assert!(mock_client.receive_stream().await.is_ok());

    // Assert streams are available
    assert!(!streamer.get_available_streams().await.is_empty());

    // Test error handling: invalid segment
    let invalid_segment = Segment::invalid();
    assert!(streamer.add_segment(invalid_segment).await.is_err());

    // Cleanup
    main_engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_engine_error_recovery() {
    let main_engine = MainEngine::new(Default::default()).await.unwrap();
    main_engine.start().await.unwrap();

    // Simulate error in segmenter
    let segmenter = main_engine.get_segmenter().unwrap();
    assert!(segmenter.perform_invalid_segmentation().await.is_err());

    // Check recovery
    assert!(segmenter.is_healthy().await);

    main_engine.shutdown().await.unwrap();
}