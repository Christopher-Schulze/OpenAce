//! Segmenter Engine for OpenAce Rust
//!
//! This module implements the segmenter engine that handles media segmentation,
//! similar to the C version but with modern Rust async/await patterns.

use crate::error::{Result, OpenAceError};
use crate::config::SegmenterConfig;
use crate::core::traits::*;
use crate::core::types::*;
use crate::utils::threading::{SafeMutex, SafeRwLock};
use crate::engines::EngineHealth;

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, info, error, trace, instrument};
use serde::{Serialize, Deserialize};

/// Segmenter engine state
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SegmenterState {
    Uninitialized,
    Initializing,
    Ready,
    Segmenting,
    Paused,
    Error(String),
    Shutdown,
}

/// Segmentation job information
#[derive(Debug, Clone)]
pub struct SegmentationJob {
    pub id: Id,
    pub input_path: PathBuf,
    pub output_dir: PathBuf,
    pub segment_duration: Duration,
    pub format: MediaFormat,
    pub quality: MediaQuality,
    pub created_at: Instant,
    pub status: JobStatus,
    pub progress: f32,
    pub error_message: Option<String>,
}

/// Job status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum JobStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

/// Segmentation statistics
#[derive(Debug, Clone)]
pub struct SegmenterStats {
    pub total_jobs: u64,
    pub completed_jobs: u64,
    pub failed_jobs: u64,
    pub cancelled_jobs: u64,
    pub active_jobs: u64,
    pub total_segments_created: u64,
    pub total_bytes_processed: u64,
    pub average_processing_time: Duration,
    pub last_job_time: Option<Instant>,
    pub uptime: Duration,
    pub memory_usage: u64,
}

impl Default for SegmenterStats {
    fn default() -> Self {
        Self {
            total_jobs: 0,
            completed_jobs: 0,
            failed_jobs: 0,
            cancelled_jobs: 0,
            active_jobs: 0,
            total_segments_created: 0,
            total_bytes_processed: 0,
            average_processing_time: Duration::ZERO,
            last_job_time: None,
            uptime: Duration::ZERO,
            memory_usage: 0,
        }
    }
}

/// Segmenter engine implementation
#[derive(Debug)]
pub struct SegmenterEngine {
    config: SafeRwLock<SegmenterConfig>,
    state: SafeRwLock<SegmenterState>,
    stats: SafeRwLock<SegmenterStats>,
    jobs: SafeRwLock<HashMap<Id, SegmentationJob>>,
    job_sender: SafeMutex<Option<mpsc::UnboundedSender<SegmentationJob>>>,
    shutdown_sender: SafeMutex<Option<oneshot::Sender<()>>>,
    start_time: Instant,
}

impl SegmenterEngine {
    /// Create a new segmenter engine
    pub fn new(config: &SegmenterConfig) -> Result<Self> {
        info!("Creating segmenter engine");
        
        Ok(Self {
            config: SafeRwLock::new(config.clone(), "segmenter_config"),
            state: SafeRwLock::new(SegmenterState::Uninitialized, "segmenter_state"),
            stats: SafeRwLock::new(SegmenterStats::default(), "segmenter_stats"),
            jobs: SafeRwLock::new(HashMap::new(), "segmenter_jobs"),
            job_sender: SafeMutex::new(None, "segmenter_job_sender"),
            shutdown_sender: SafeMutex::new(None, "segmenter_shutdown_sender"),
            start_time: Instant::now(),
        })
    }
    
    /// Initialize the segmenter engine
    pub async fn initialize(&self) -> Result<()> {
        trace!("Initializing SegmenterEngine");
        info!("Initializing segmenter engine");
        
        {
            let mut state = self.state.write().await;
            debug!("Current segmenter state: {:?}", *state);
            if *state != SegmenterState::Uninitialized {
            debug!("Segmenter engine already initialized, returning error");
            return Err(OpenAceError::invalid_state_transition(
                format!("{:?}", *state),
                "Uninitialized"
            ));
            }
            debug!("Setting segmenter state to Initializing");
            *state = SegmenterState::Initializing;
        }
        
        // Create job processing channel
        debug!("Creating communication channels for segmenter");
        let (job_tx, job_rx) = mpsc::unbounded_channel();
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        
        // Store senders
        debug!("Storing job and shutdown senders");
        *self.job_sender.lock().await = Some(job_tx);
        *self.shutdown_sender.lock().await = Some(shutdown_tx);
        
        // Start job processing task
        debug!("Starting job processing task for segmenter");
        let engine_ref = self.clone_for_task();
        tokio::spawn(async move {
            engine_ref.job_processing_loop(job_rx, shutdown_rx).await;
        });
        
        // Update state
        debug!("Setting segmenter state to Ready");
        *self.state.write().await = SegmenterState::Ready;
        
        info!("Segmenter engine initialized successfully");
        Ok(())
    }
    
    /// Shutdown the segmenter engine
    pub async fn shutdown(&self) -> Result<()> {
        trace!("Shutting down SegmenterEngine");
        info!("Shutting down segmenter engine");
        
        // Update state
        debug!("Setting segmenter state to Shutdown");
        *self.state.write().await = SegmenterState::Shutdown;
        
        // Send shutdown signal
        debug!("Sending shutdown signal to segmenter job processing loop");
        if let Some(sender) = self.shutdown_sender.lock().await.take() {
            debug!("Shutdown sender found, sending signal");
            let _ = sender.send(());
        } else {
            debug!("No shutdown sender found, skipping signal");
        }
        
        // Cancel all pending jobs
        debug!("Cancelling all pending segmentation jobs");
        self.cancel_all_jobs().await?;
        
        info!("Segmenter engine shut down successfully");
        Ok(())
    }
    
    /// Pause the segmenter engine
    pub async fn pause(&self) -> Result<()> {
        trace!("Pausing SegmenterEngine");
        info!("Pausing segmenter engine");
        
        let mut state = self.state.write().await;
        debug!("Current segmenter state for pause: {:?}", *state);
        match *state {
            SegmenterState::Ready | SegmenterState::Segmenting => {
                debug!("Segmenter state allows pausing, setting to Paused");
                *state = SegmenterState::Paused;
                Ok(())
            }
            _ => {
                debug!("Cannot pause segmenter engine in current state: {:?}", *state);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Ready or Segmenting"
                ))
            }
        }
    }
    
    /// Resume the segmenter engine
    pub async fn resume(&self) -> Result<()> {
        trace!("Resuming SegmenterEngine");
        info!("Resuming segmenter engine");
        
        let mut state = self.state.write().await;
        debug!("Current segmenter state for resume: {:?}", *state);
        match *state {
            SegmenterState::Paused => {
                debug!("Segmenter is paused, setting to Ready");
                *state = SegmenterState::Ready;
                Ok(())
            }
            _ => {
                debug!("Cannot resume segmenter engine in current state: {:?}", *state);
                Err(OpenAceError::invalid_state_transition(
                    format!("{:?}", *state),
                    "Paused"
                ))
            }
        }
    }
    
    /// Submit a segmentation job
    #[instrument(skip(self))]
    pub async fn submit_job(
        &self,
        input_path: PathBuf,
        output_dir: PathBuf,
        segment_duration: Duration,
        format: MediaFormat,
        quality: MediaQuality,
    ) -> Result<Id> {
        trace!("Submitting segmentation job");
        debug!("Submitting new segmentation job for input: {:?}", input_path);
        let state = self.state.read().await;
        debug!("Current segmenter state for job submission: {:?}", *state);
        if !matches!(*state, SegmenterState::Ready | SegmenterState::Segmenting) {
            debug!("Segmenter engine not ready to accept jobs, current state: {:?}", *state);
            return Err(OpenAceError::invalid_state_transition(
                format!("{:?}", *state),
                "Ready or Segmenting"
            ));
        }
        drop(state);
        
        let job_id = Id::generate();
        debug!("Generated job ID: {}", job_id);
        let job = SegmentationJob {
            id: job_id.clone(),
            input_path: input_path.clone(),
            output_dir: output_dir.clone(),
            segment_duration,
            format,
            quality,
            created_at: Instant::now(),
            status: JobStatus::Pending,
            progress: 0.0,
            error_message: None,
        };
        
        // Store job
        debug!("Storing segmentation job in jobs map");
        trace!("Job details: id={}, input_path={:?}, output_dir={:?}, segment_duration={:?}, format={:?}, quality={:?}", 
               job_id, input_path, output_dir, segment_duration, format, quality);
        self.jobs.write().await.insert(job_id.clone(), job.clone());
        info!("Segmentation job stored successfully: id={}", job_id);
        
        // Send job to processing queue
        debug!("Sending job to processing queue");
        trace!("Attempting to send job {} to processing queue", job_id);
        if let Some(sender) = self.job_sender.lock().await.as_ref() {
            debug!("Job sender available, sending job to queue");
            sender.send(job).map_err(|_| {
                error!("Failed to send job {} to processing queue", job_id);
                OpenAceError::engine_operation("segmenter", "queue_job", "Failed to queue segmentation job")
            })?;
            info!("Job {} successfully queued for processing", job_id);
        } else {
            error!("Job processing queue is not available for job {}", job_id);
            return Err(OpenAceError::engine_operation("segmenter", "queue_job", "Job processing channel not available"));
        }
        
        // Update stats
        debug!("Updating segmenter statistics for new job");
        trace!("Incrementing total_jobs and active_jobs counters");
        {
            let mut stats = self.stats.write().await;
            stats.total_jobs += 1;
            stats.active_jobs += 1;
            stats.last_job_time = Some(Instant::now());
            info!("Updated stats - total jobs: {}, active jobs: {}", stats.total_jobs, stats.active_jobs);
        }
        
        info!("Segmentation job submitted successfully: id={}, input={:?}", job_id, input_path);
        Ok(job_id)
    }
    
    /// Cancel a segmentation job
    pub async fn cancel_job(&self, job_id: &Id) -> Result<bool> {
        trace!("Cancelling segmentation job: {}", job_id);
        debug!("Attempting to cancel segmentation job: {}", job_id);
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            debug!("Found job with status: {:?}", job.status);
            if matches!(job.status, JobStatus::Pending | JobStatus::Processing) {
                debug!("Job can be cancelled, updating status to Cancelled");
                job.status = JobStatus::Cancelled;
                
                // Update stats
                debug!("Updating segmenter statistics for cancelled job");
                let mut stats = self.stats.write().await;
                stats.cancelled_jobs += 1;
                if stats.active_jobs > 0 {
                    stats.active_jobs -= 1;
                }
                debug!("Updated stats - cancelled jobs: {}, active jobs: {}", stats.cancelled_jobs, stats.active_jobs);
                
                info!("Segmentation job cancelled: {}", job_id);
                Ok(true)
            } else {
                debug!("Job cannot be cancelled in current status: {:?}", job.status);
                Ok(false)
            }
        } else {
            debug!("Job not found for cancellation: {}", job_id);
            Err(OpenAceError::not_found(format!("Job not found: {}", job_id)))
        }
    }
    
    /// Cancel all jobs
    async fn cancel_all_jobs(&self) -> Result<()> {
        debug!("Cancelling all pending and processing segmentation jobs");
        let mut jobs = self.jobs.write().await;
        let mut cancelled_count = 0;
        
        debug!("Checking {} jobs for cancellation", jobs.len());
        for job in jobs.values_mut() {
            if matches!(job.status, JobStatus::Pending | JobStatus::Processing) {
                debug!("Cancelling job: {} with status: {:?}", job.id, job.status);
                job.status = JobStatus::Cancelled;
                cancelled_count += 1;
            }
        }
        
        // Update stats
        if cancelled_count > 0 {
            debug!("Updating segmenter statistics for {} cancelled jobs", cancelled_count);
            let mut stats = self.stats.write().await;
            stats.cancelled_jobs += cancelled_count;
            stats.active_jobs = stats.active_jobs.saturating_sub(cancelled_count);
            debug!("Updated stats - cancelled jobs: {}, active jobs: {}", stats.cancelled_jobs, stats.active_jobs);
        } else {
            debug!("No jobs were cancelled");
        }
        
        info!("Cancelled {} segmentation jobs", cancelled_count);
        Ok(())
    }
    
    /// Get job status
    pub async fn get_job_status(&self, job_id: &Id) -> Result<SegmentationJob> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id)
            .cloned()
            .ok_or_else(|| OpenAceError::not_found(format!("Job not found: {}", job_id)))
    }
    
    /// Get all jobs
    pub async fn get_all_jobs(&self) -> Vec<SegmentationJob> {
        self.jobs.read().await.values().cloned().collect()
    }
    
    /// Get active jobs
    pub async fn get_active_jobs(&self) -> Vec<SegmentationJob> {
        self.jobs.read().await
            .values()
            .filter(|job| matches!(job.status, JobStatus::Pending | JobStatus::Processing))
            .cloned()
            .collect()
    }
    
    /// Update configuration
    pub async fn update_config(&self, config: &SegmenterConfig) -> Result<()> {
        trace!("Updating segmenter configuration");
        info!("Updating segmenter configuration");
        *self.config.write().await = config.clone();
        Ok(())
    }
    
    /// Get health status
    pub async fn get_health(&self) -> EngineHealth {
        trace!("Getting segmenter health");
        let state = self.state.read().await;
        let stats = self.stats.read().await;
        
        match *state {
            SegmenterState::Ready | SegmenterState::Segmenting => {
                if stats.active_jobs > 100 {
                    EngineHealth::Warning("High job queue load".to_string())
                } else {
                    EngineHealth::Healthy
                }
            }
            SegmenterState::Error(ref msg) => {
                EngineHealth::Error(msg.clone())
            }
            SegmenterState::Paused => {
                EngineHealth::Warning("Engine is paused".to_string())
            }
            _ => EngineHealth::Unknown,
        }
    }
    
    /// Clone for task (simplified clone for async tasks)
    fn clone_for_task(&self) -> SegmenterEngineTask {
        SegmenterEngineTask {
            config: Arc::clone(&self.config.inner()),
            state: Arc::clone(&self.state.inner()),
            stats: Arc::clone(&self.stats.inner()),
            jobs: Arc::clone(&self.jobs.inner()),
        }
    }
    
    /// Job processing loop
    #[allow(dead_code)]
    async fn job_processing_loop(
        &self,
        mut job_rx: mpsc::UnboundedReceiver<SegmentationJob>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        info!("Starting segmentation job processing loop");
        
        loop {
            tokio::select! {
                job = job_rx.recv() => {
                    if let Some(job) = job {
                        self.process_job(job).await;
                    } else {
                        break;
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for job processing loop");
                    break;
                }
            }
        }
        
        info!("Segmentation job processing loop stopped");
    }
    
    /// Process a single segmentation job
    #[instrument(skip(self))]
    #[allow(dead_code)]
    async fn process_job(&self, mut job: SegmentationJob) {
        trace!("Processing segmentation job: {}", job.id);
        info!("Processing segmentation job: {}", job.id);
        
        // Check if job was cancelled
        {
            let jobs = self.jobs.read().await;
            if let Some(current_job) = jobs.get(&job.id) {
                if current_job.status == JobStatus::Cancelled {
                    return;
                }
            }
        }
        
        // Update job status to processing
        job.status = JobStatus::Processing;
        self.update_job_status(&job).await;
        
        // Update engine state
        *self.state.write().await = SegmenterState::Segmenting;
        
        let start_time = Instant::now();
        let result = self.perform_segmentation(&mut job).await;
        let processing_time = start_time.elapsed();
        
        // Update job based on result
        match result {
            Ok(_) => {
                job.status = JobStatus::Completed;
                job.progress = 1.0;
                
                // Update stats
                let mut stats = self.stats.write().await;
                stats.completed_jobs += 1;
                stats.active_jobs = stats.active_jobs.saturating_sub(1);
                stats.last_job_time = Some(Instant::now());
                
                // Update average processing time
                let total_time = stats.average_processing_time.as_secs_f64() * (stats.completed_jobs - 1) as f64
                    + processing_time.as_secs_f64();
                stats.average_processing_time = Duration::from_secs_f64(total_time / stats.completed_jobs as f64);
                
                info!("Segmentation job completed: {} (took {:?})", job.id, processing_time);
            }
            Err(e) => {
                job.status = JobStatus::Failed;
                job.error_message = Some(e.to_string());
                
                // Update stats
                let mut stats = self.stats.write().await;
                stats.failed_jobs += 1;
                stats.active_jobs = stats.active_jobs.saturating_sub(1);
                
                error!("Segmentation job failed: {} - {}", job.id, e);
            }
        }
        
        // Update job status
        self.update_job_status(&job).await;
        
        // Update engine state back to ready if no more active jobs
        let active_jobs = self.stats.read().await.active_jobs;
        if active_jobs == 0 {
            *self.state.write().await = SegmenterState::Ready;
        }
    }
    
    /// Update job status in storage
    #[allow(dead_code)]
    async fn update_job_status(&self, job: &SegmentationJob) {
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id.clone(), job.clone());
    }
    
    /// Perform the actual segmentation
    async fn perform_segmentation(&self, job: &mut SegmentationJob) -> Result<()> {
        
        use tokio::process::Command as TokioCommand;
        use std::fs;
        

        trace!("Performing segmentation for job: {}", job.id);
        debug!("Starting segmentation for job: {}", job.id);
        debug!("Input path: {:?}, Output dir: {:?}", job.input_path, job.output_dir);
        debug!("Segment duration: {:?}, Format: {:?}, Quality: {:?}", job.segment_duration, job.format, job.quality);

        // 1. Validate input file
        if !job.input_path.exists() || !job.input_path.is_file() {
            return Err(OpenAceError::invalid_argument("Input file does not exist or is not a file".to_string()));
        }

        // 2. Create output directory
        fs::create_dir_all(&job.output_dir)?;

        // 3. Prepare FFmpeg command
        let segment_time = job.segment_duration.as_secs() as u32;
        let output_format = match job.format {
            MediaFormat::Mp4 => "mp4",
            _ => return Err(OpenAceError::InvalidArgument("Unsupported media format".to_string())),
        };
        let quality_param = match job.quality {
            MediaQuality::High => "18",
            MediaQuality::Medium => "24",
            MediaQuality::Low => "30",
            MediaQuality::UltraHigh => "12",
        };

        let output_pattern = job.output_dir.join("segment_%03d").with_extension(output_format);

        let mut cmd = TokioCommand::new("ffmpeg");
        cmd.arg("-i").arg(&job.input_path);
        cmd.arg("-c:v").arg("libx264");
        cmd.arg("-crf").arg(quality_param);
        cmd.arg("-c:a").arg("aac");
        cmd.arg("-f").arg("segment");
        cmd.arg("-segment_time").arg(segment_time.to_string());
        cmd.arg("-reset_timestamps").arg("1");
        cmd.arg(output_pattern.to_str().unwrap());

        // Execute FFmpeg
        let output = cmd.output().await?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(OpenAceError::engine_operation("segmenter", "ffmpeg", &error_msg));
        }

        // Simulate progress (in real scenario, parse FFmpeg output for progress)
        // For now, we'll simulate with steps
        let total_steps = 10;
        for i in 1..=total_steps {
            // Check cancellation
            {
                let jobs = self.jobs.read().await;
                if let Some(current_job) = jobs.get(&job.id) {
                    if current_job.status == JobStatus::Cancelled {
                        // Attempt to kill the process if possible, but since it's already awaited, handle post-facto
                        return Err(OpenAceError::operation_cancelled("Job was cancelled"));
                    }
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await; // Simulate time per step
            job.progress = i as f32 / total_steps as f32;
            self.update_job_status(job).await;
        }

        // Update stats (count segments, bytes, etc.)
        let mut stats = self.stats.write().await;
        // For simplicity, assume 10 segments
        stats.total_segments_created += 10;
        stats.total_bytes_processed += 1000000; // Dummy value

        debug!("Segmentation completed for job: {}", job.id);
        Ok(())
    }
}

/// Simplified engine reference for async tasks
#[derive(Debug, Clone)]
struct SegmenterEngineTask {
    #[allow(dead_code)]
    config: Arc<tokio::sync::RwLock<SegmenterConfig>>,
    state: Arc<tokio::sync::RwLock<SegmenterState>>,
    stats: Arc<tokio::sync::RwLock<SegmenterStats>>,
    jobs: Arc<tokio::sync::RwLock<HashMap<Id, SegmentationJob>>>,
}

impl SegmenterEngineTask {
    /// Job processing loop
    async fn job_processing_loop(
        &self,
        mut job_rx: mpsc::UnboundedReceiver<SegmentationJob>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        info!("Starting segmentation job processing loop");
        debug!("Segmenter job processing loop initialized");
        
        loop {
            tokio::select! {
                job = job_rx.recv() => {
                    if let Some(job) = job {
                        debug!("Received job for processing: {}", job.id);
                        self.process_job(job).await;
                    } else {
                        debug!("Job receiver channel closed, breaking loop");
                        break;
                    }
                }
                _ = &mut shutdown_rx => {
                    info!("Received shutdown signal for job processing loop");
                    debug!("Shutdown signal received, breaking processing loop");
                    break;
                }
            }
        }
        
        info!("Segmentation job processing loop stopped");
        debug!("Job processing loop cleanup completed");
    }
    
    /// Process a single segmentation job
    #[instrument(skip(self))]
    async fn process_job(&self, mut job: SegmentationJob) {
        info!("Processing segmentation job: {}", job.id);
        debug!("Starting job processing for: {}", job.id);
        
        // Check if job was cancelled
        {
            debug!("Checking if job was cancelled before processing");
            let jobs = self.jobs.read().await;
            if let Some(current_job) = jobs.get(&job.id) {
                if current_job.status == JobStatus::Cancelled {
                    debug!("Job {} was cancelled, skipping processing", job.id);
                    return;
                }
            }
        }
        
        // Update job status to processing
        debug!("Updating job status to Processing");
        job.status = JobStatus::Processing;
        self.update_job_status(&job).await;
        
        // Update engine state
        debug!("Setting segmenter engine state to Segmenting");
        *self.state.write().await = SegmenterState::Segmenting;
        
        debug!("Starting segmentation process for job: {}", job.id);
        let start_time = Instant::now();
        let result = self.perform_segmentation(&mut job).await;
        let processing_time = start_time.elapsed();
        debug!("Segmentation process completed in {:?}", processing_time);
        
        // Update job based on result
        match result {
            Ok(_) => {
                debug!("Job {} completed successfully", job.id);
                job.status = JobStatus::Completed;
                job.progress = 1.0;
                
                // Update stats
                debug!("Updating statistics for completed job");
                let mut stats = self.stats.write().await;
                stats.completed_jobs += 1;
                stats.active_jobs = stats.active_jobs.saturating_sub(1);
                stats.last_job_time = Some(Instant::now());
                
                // Update average processing time
                let total_time = stats.average_processing_time.as_secs_f64() * (stats.completed_jobs - 1) as f64
                    + processing_time.as_secs_f64();
                stats.average_processing_time = Duration::from_secs_f64(total_time / stats.completed_jobs as f64);
                debug!("Updated stats - completed: {}, active: {}, avg time: {:?}", 
                       stats.completed_jobs, stats.active_jobs, stats.average_processing_time);
                
                info!("Segmentation job completed: {} (took {:?})", job.id, processing_time);
            }
            Err(e) => {
                debug!("Job {} failed with error: {}", job.id, e);
                job.status = JobStatus::Failed;
                job.error_message = Some(e.to_string());
                
                // Update stats
                debug!("Updating statistics for failed job");
                let mut stats = self.stats.write().await;
                stats.failed_jobs += 1;
                stats.active_jobs = stats.active_jobs.saturating_sub(1);
                debug!("Updated stats - failed: {}, active: {}", stats.failed_jobs, stats.active_jobs);
                
                error!("Segmentation job failed: {} - {}", job.id, e);
            }
        }
        
        // Update job status
        debug!("Updating final job status in storage");
        self.update_job_status(&job).await;
        
        // Update engine state back to ready if no more active jobs
        let active_jobs = self.stats.read().await.active_jobs;
        debug!("Checking if engine should return to Ready state, active jobs: {}", active_jobs);
        if active_jobs == 0 {
            debug!("No more active jobs, setting engine state to Ready");
            *self.state.write().await = SegmenterState::Ready;
        }
    }
    
    /// Update job status in storage
    async fn update_job_status(&self, job: &SegmentationJob) {
        debug!("Updating job status in storage for job: {} to {:?}", job.id, job.status);
        let mut jobs = self.jobs.write().await;
        jobs.insert(job.id.clone(), job.clone());
        debug!("Job status updated successfully");
    }
    
    /// Perform the actual segmentation
    async fn perform_segmentation(&self, job: &mut SegmentationJob) -> Result<()> {
        // This is a placeholder for the actual segmentation logic
        // In a real implementation, this would:
        // 1. Validate input file
        // 2. Create output directory
        // 3. Use FFmpeg or similar to segment the media
        // 4. Update progress during segmentation
        // 5. Handle errors and cleanup
        
        debug!("Starting segmentation for job: {}", job.id);
        debug!("Input path: {:?}, Output dir: {:?}", job.input_path, job.output_dir);
        debug!("Segment duration: {:?}, Format: {:?}, Quality: {:?}", 
               job.segment_duration, job.format, job.quality);
        
        // Simulate segmentation work with progress updates
        for i in 1..=10 {
            // Check if job was cancelled
            {
                debug!("Checking cancellation status for job: {} (step {})", job.id, i);
                let jobs = self.jobs.read().await;
                if let Some(current_job) = jobs.get(&job.id) {
                    if current_job.status == JobStatus::Cancelled {
                        debug!("Job {} was cancelled during processing at step {}", job.id, i);
                        return Err(OpenAceError::operation_cancelled("Job was cancelled"));
                    }
                }
            }
            
            // Simulate work
            debug!("Processing segment {} of 10 for job: {}", i, job.id);
            tokio::time::sleep(Duration::from_millis(100)).await;
            
            // Update progress
            job.progress = i as f32 / 10.0;
            debug!("Job {} progress updated to: {:.1}%", job.id, job.progress * 100.0);
            self.update_job_status(job).await;
        }
        
        debug!("Segmentation completed for job: {}", job.id);
        Ok(())
    }
}

#[async_trait]
impl Lifecycle for SegmenterEngine {
    async fn initialize(&mut self) -> Result<()> {
        self.initialize().await
    }
    
    async fn shutdown(&mut self) -> Result<()> {
        self.shutdown().await
    }
    
    fn is_initialized(&self) -> bool {
        // For sync check, we use try_read
        self.state.try_read()
            .map(|state| !matches!(*state, SegmenterState::Uninitialized))
            .unwrap_or(false)
    }
}

#[async_trait]
impl Pausable for SegmenterEngine {
    async fn pause(&mut self) -> Result<()> {
        self.pause().await
    }
    
    async fn resume(&mut self) -> Result<()> {
        self.resume().await
    }
    
    async fn is_paused(&self) -> bool {
        self.state.try_read()
            .map(|state| matches!(*state, SegmenterState::Paused))
            .unwrap_or(false)
    }
}

#[async_trait]
impl StatisticsProvider for SegmenterEngine {
    type Stats = SegmenterStats;
    
    async fn get_statistics(&self) -> Self::Stats {
        let mut stats = self.stats.try_read()
            .map(|s| s.clone())
            .unwrap_or_default();
        
        // Update uptime
        stats.uptime = self.start_time.elapsed();
        stats
    }
    
    async fn reset_statistics(&mut self) -> Result<()> {
        if let Ok(mut stats) = self.stats.try_write() {
            *stats = SegmenterStats::default();
        }
        Ok(())
    }
}

#[async_trait]
impl Monitorable for SegmenterEngine {
    type Health = EngineHealth;
    
    async fn get_health(&self) -> Self::Health {
        if let Ok(state) = self.state.try_read() {
            match *state {
                SegmenterState::Ready | SegmenterState::Segmenting => {
                    if let Ok(stats) = self.stats.try_read() {
                        if stats.active_jobs > 100 {
                            EngineHealth::Warning("High job queue load".to_string())
                        } else {
                            EngineHealth::Healthy
                        }
                    } else {
                        EngineHealth::Warning("Cannot read statistics".to_string())
                    }
                }
                SegmenterState::Error(ref msg) => EngineHealth::Error(msg.clone()),
                SegmenterState::Paused => EngineHealth::Warning("Engine is paused".to_string()),
                _ => EngineHealth::Unknown,
            }
        } else {
            EngineHealth::Error("Cannot read engine state".to_string())
        }
    }
    
    async fn health_check(&self) -> Result<Self::Health> {
        Ok(self.get_health().await)
    }
}

#[async_trait]
impl Configurable for SegmenterEngine {
    type Config = SegmenterConfig;
    
    async fn get_config(&self) -> Self::Config {
        self.config.try_read()
            .map(|c| c.clone())
            .unwrap_or_default()
    }
    
    async fn update_config(&mut self, config: &Self::Config) -> Result<()> {
        let mut current_config = self.config.write().await;
        *current_config = config.clone();
        Ok(())
    }
    
    async fn validate_config(config: &Self::Config) -> Result<()> {
        // Validate segmenter configuration
        if config.segment_duration_seconds <= 0.0 {
            return Err(OpenAceError::configuration(
                "segment_duration_seconds must be greater than 0"
            ));
        }

        if config.max_segment_size_bytes == 0 {
            return Err(OpenAceError::configuration(
                "max_segment_size_bytes must be greater than 0"
            ));
        }
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SegmenterConfig;
    use std::time::Duration;
    
    #[tokio::test]
    async fn test_segmenter_engine_creation() {
        let config = SegmenterConfig::default();
        let engine = SegmenterEngine::new(&config);
        assert!(engine.is_ok());
    }
    
    #[tokio::test]
    async fn test_segmenter_engine_lifecycle() {
        let config = SegmenterConfig::default();
        let engine = SegmenterEngine::new(&config).unwrap();
        
        assert!(!engine.is_initialized());
        
        // Initialize
        let result = engine.initialize().await;
        assert!(result.is_ok());
        assert!(engine.is_initialized());
        
        // Shutdown
        let result = engine.shutdown().await;
        assert!(result.is_ok());
    }
    
    #[tokio::test]
    async fn test_segmenter_job_submission() {
        let config = SegmenterConfig::default();
        let engine = SegmenterEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let job_id = engine.submit_job(
            PathBuf::from("/test/input.mp4"),
            PathBuf::from("/test/output"),
            Duration::from_secs(10),
            MediaFormat::Mp4,
            MediaQuality::High,
        ).await;
        
        assert!(job_id.is_ok());
        
        let job_id = job_id.unwrap();
        let job_status = engine.get_job_status(&job_id).await;
        assert!(job_status.is_ok());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_segmenter_job_cancellation() {
        let config = SegmenterConfig::default();
        let engine = SegmenterEngine::new(&config).unwrap();
        engine.initialize().await.unwrap();
        
        let job_id = engine.submit_job(
            PathBuf::from("/test/input.mp4"),
            PathBuf::from("/test/output"),
            Duration::from_secs(10),
            MediaFormat::Mp4,
            MediaQuality::High,
        ).await.unwrap();
        
        let cancelled = engine.cancel_job(&job_id).await;
        assert!(cancelled.is_ok());
        assert!(cancelled.unwrap());
        
        engine.shutdown().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_segmenter_statistics() {
        let config = SegmenterConfig::default();
        let mut engine = SegmenterEngine::new(&config).unwrap();
        
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_jobs, 0);
        assert_eq!(stats.completed_jobs, 0);
        assert_eq!(stats.failed_jobs, 0);
        
        engine.reset_statistics().await.unwrap();
        let stats = engine.get_statistics().await;
        assert_eq!(stats.total_jobs, 0);
    }
}