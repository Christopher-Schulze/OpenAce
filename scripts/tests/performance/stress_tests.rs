use openace_rust::engines::{MainEngine};
use tokio::test;
use tokio::time::{sleep, Duration};
use std::sync::Arc;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn stress_test_multiple_engines() {
    let engine = Arc::new(MainEngine::new(Default::default()).await.unwrap());
    engine.start().await.unwrap();

    let mut handles = vec![];
    for _ in 0..100 {
        let engine_clone = engine.clone();
        handles.push(tokio::spawn(async move {
            // Simulate heavy load: multiple segmentations and streaming requests
            if let Some(segmenter) = engine_clone.get_segmenter() {
                let _ = segmenter.perform_segmentation(/* params */).await;
            }
            sleep(Duration::from_millis(10)).await;
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn stress_test_long_running_operations() {
    let engine = Arc::new(MainEngine::new(Default::default()).await.unwrap());
    engine.start().await.unwrap();

    let mut handles = vec![];
    for _ in 0..50 {
        let engine_clone = engine.clone();
        handles.push(tokio::spawn(async move {
            // Simulate long-running segmentation
            if let Some(segmenter) = engine_clone.get_segmenter() {
                let input = PathBuf::from("tests/fixtures/test_media/large_sample.mp4");
                let output = PathBuf::from("tests/fixtures/output_segments");
                let _ = segmenter.perform_segmentation(input, output, 10, "mp4".to_string(), "high".to_string()).await;
            }
            sleep(Duration::from_secs(5)).await;
        }));
    }

    for handle in handles {
        handle.await.unwrap();
    }

    engine.shutdown().await.unwrap();
}

#[tokio::test]
async fn benchmark_segmentation_performance() {
    let segmenter = SegmenterEngine::new(Default::default()).await.unwrap();
    let input = PathBuf::from("tests/fixtures/test_media/sample.mp4");
    let output = PathBuf::from("tests/fixtures/output_segments");

    let start = Instant::now();
    let _ = segmenter.perform_segmentation(input, output, 10, "mp4".to_string(), "high".to_string()).await.unwrap();
    let duration = start.elapsed();

    assert!(duration < Duration::from_secs(30)); // Example threshold
}

#[tokio::test]
async fn benchmark_streaming_performance() {
    let (tx, _rx) = mpsc::channel(32);
    let streamer = StreamerEngine::new(tx).await.unwrap();

    let start = Instant::now();
    for i in 0..1000 {
        streamer.add_segment(format!("segment_{}", i)).await.unwrap();
    }
    let duration = start.elapsed();

    assert!(duration < Duration::from_secs(10)); // Example threshold
}