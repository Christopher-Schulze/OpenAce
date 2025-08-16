use criterion::{criterion_group, criterion_main, Criterion};
use openace_rust::engines::segmenter::SegmenterEngine;
use openace_rust::config::SegmenterConfig;
use std::path::PathBuf;
use tokio::runtime::Runtime;

fn benchmark_segmentation(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    c.bench_function("segmentation_performance", |b| {
        b.iter(|| {
            rt.block_on(async {
                let config = SegmenterConfig::default();
                let engine = SegmenterEngine::new(&config).unwrap();
                let input_path = PathBuf::from("tests/fixtures/test_media/sample.mp4");
                let output_dir = PathBuf::from("tests/fixtures/output_segments");
                let segment_duration = 10;
                let format = "mp4".to_string();
                let quality = "high".to_string();
                
                // Use a public method or create a mock segmentation task
                let _result = engine.submit_job(input_path, output_dir, segment_duration, format, quality).await;
            })
        });
    });
}

criterion_group!(benches, benchmark_segmentation);
criterion_main!(benches);