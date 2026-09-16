//! Bounded batch execution (spec §§12–13, ADR 007).
//!
//! Tokio `rt-multi-thread` + semaphore-bounded `spawn_blocking`: CPU work
//! (decode/encode) stays off the async reactor. Per-file results are
//! independent; cancellation is cooperative; memory stays bounded because
//! at most `concurrency` files are in flight and each file streams
//! decode → transform → encode → write → release.

use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

use forge_core::{
    CancelToken, ConversionRequest, ForgeError, JobEventSink, JobId, JobProgress, NeverCancel,
    Result,
};
use tokio::sync::Semaphore;

/// Batch tuning knobs.
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Max files in flight (default: CPU count, min 1).
    pub concurrency: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            concurrency: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
                .max(1),
        }
    }
}

/// Aggregate outcome: exact counts, never "something went wrong".
#[derive(Debug)]
pub struct BatchReport {
    /// Successfully written outputs.
    pub outputs: Vec<PathBuf>,
    /// `input: error` per failed file.
    pub failures: Vec<String>,
    /// Files skipped by collision policy.
    pub skipped: Vec<PathBuf>,
}

/// Shared cancellation flag (cloneable across tasks).
#[derive(Debug, Default)]
pub struct CancelFlag {
    flag: AtomicBool,
}

impl CancelFlag {
    /// New, uncancelled flag.
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Request cancellation.
    pub fn cancel(&self) {
        self.flag.store(true, Ordering::SeqCst);
    }
}

impl CancelToken for CancelFlag {
    fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }
}


/// One file's work unit (must be `Send + 'static` for `spawn_blocking`).
pub trait BatchTask: Fn(PathBuf) -> Result<Option<PathBuf>> + Send + Sync + 'static {}
impl<F> BatchTask for F where F: Fn(PathBuf) -> Result<Option<PathBuf>> + Send + Sync + 'static {}

/// Run `inputs` through `task` with bounded concurrency.
///
/// - `Ok(Some(path))` → success; `Ok(None)` → skipped; `Err(Cancelled)` →
///   stop promptly; other `Err` → recorded, batch continues.
/// - Progress: per-file + aggregate fraction via `sink`.
/// - Returns [`BatchReport`] with exact counts.
pub async fn run_batch<F>(
    inputs: Vec<PathBuf>,
    task: F,
    config: &BatchConfig,
    sink: Arc<dyn JobEventSink>,
    job_id: &JobId,
    cancel: Arc<CancelFlag>,
) -> Result<BatchReport>
where
    F: BatchTask,
{
    if inputs.is_empty() {
        return Err(ForgeError::InvalidConfiguration(
            "batch needs at least one input".to_string(),
        ));
    }
    let total = inputs.len();
    let semaphore = Arc::new(Semaphore::new(config.concurrency.max(1)));
    let completed = Arc::new(AtomicUsize::new(0));
    let task = Arc::new(task);

    let mut handles = Vec::with_capacity(total);
    for input in inputs {
        if cancel.is_cancelled() {
            break;
        }
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| ForgeError::Cancelled)?;
        let task = task.clone();
        let sink = sink.clone();
        let job_id = job_id.clone();
        let completed = completed.clone();
        let cancel = cancel.clone();
        handles.push(tokio::task::spawn_blocking(move || {
            let _permit = permit; // hold until task end → bounds concurrency
            if cancel.is_cancelled() {
                return (input, Err(ForgeError::Cancelled));
            }
            let outcome = task(input.clone());
            let done = completed.fetch_add(1, Ordering::SeqCst) + 1;
            sink.progress(
                &job_id,
                &JobProgress::new(
                    done as f32 / total as f32,
                    Some(format!("{done}/{total}")),
                ),
            );
            (input, outcome)
        }));
    }

    let mut outputs = Vec::new();
    let mut failures = Vec::new();
    let mut skipped = Vec::new();
    for handle in handles {
        let (input, outcome) = handle
            .await
            .map_err(|e| ForgeError::InvalidConfiguration(format!("worker panic: {e}")))?;
        match outcome {
            Ok(Some(path)) => outputs.push(path),
            Ok(None) => skipped.push(input),
            Err(ForgeError::Cancelled) => {
                failures.push(format!("{}: cancelled", input.display()));
            }
            Err(e) => failures.push(format!("{}: {e}", input.display())),
        }
    }
    Ok(BatchReport {
        outputs,
        failures,
        skipped,
    })
}

/// Split one multi-input [`ConversionRequest`] into per-file requests
/// sharing options (each keeps single input + same output target).
#[must_use]
pub fn split_request(request: &ConversionRequest) -> Vec<ConversionRequest> {
    request
        .inputs
        .iter()
        .cloned()
        .map(|input| ConversionRequest {
            inputs: vec![input],
            output_format: request.output_format,
            output: request.output.clone(),
            options: request.options.clone(),
        })
        .collect()
}

/// No-op sink for headless/benchmark use.
#[derive(Debug, Default)]
pub struct NullSink;

impl JobEventSink for NullSink {
    fn progress(&self, _id: &JobId, _progress: &JobProgress) {}
    fn finished(&self, _id: &JobId, _result: &forge_core::ConversionResult) {}
    fn failed(&self, _id: &JobId, _error: &ForgeError) {}
}

/// Silence helper (kept to document the `NeverCancel` default path).
#[allow(dead_code)]
#[must_use]
pub fn never_cancel() -> NeverCancel {
    NeverCancel
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct VecSink {
        progress_calls: Mutex<Vec<f32>>,
    }
    impl JobEventSink for VecSink {
        fn progress(&self, _id: &JobId, progress: &JobProgress) {
            self.progress_calls
                .lock()
                .expect("lock")
                .push(progress.fraction);
        }
        fn finished(&self, _id: &JobId, _result: &forge_core::ConversionResult) {}
        fn failed(&self, _id: &JobId, _error: &ForgeError) {}
    }

    fn sink() -> Arc<dyn JobEventSink> {
        Arc::new(VecSink {
            progress_calls: Mutex::new(Vec::new()),
        })
    }

    #[tokio::test]
    async fn test_batch_counts_success_skip_failure() {
        let job_id = JobId::generate();
        let inputs: Vec<PathBuf> = (0..5).map(|i| PathBuf::from(format!("f{i}.png"))).collect();
        let report = run_batch(
            inputs,
            |input: PathBuf| {
                let name = input.to_string_lossy().into_owned();
                if name.contains('0') {
                    Ok(None) // skipped
                } else if name.contains('1') {
                    Err(ForgeError::InvalidFile("bad file".to_string()))
                } else {
                    Ok(Some(PathBuf::from(format!("{name}.webp"))))
                }
            },
            &BatchConfig { concurrency: 2 },
            sink(),
            &job_id,
            CancelFlag::new(),
        )
        .await
        .unwrap();
        assert_eq!(report.outputs.len(), 3);
        assert_eq!(report.skipped.len(), 1);
        assert_eq!(report.failures.len(), 1);
    }

    #[tokio::test]
    async fn test_batch_bounded_concurrency() {
        use std::sync::atomic::AtomicUsize;
        let live = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let job_id = JobId::generate();
        let inputs: Vec<PathBuf> = (0..20).map(|i| PathBuf::from(format!("f{i}.png"))).collect();
        let live_task = live.clone();
        let peak_task = peak.clone();
        run_batch(
            inputs,
            move |input: PathBuf| {
                let n = live_task.fetch_add(1, Ordering::SeqCst) + 1;
                peak_task.fetch_max(n, Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(5));
                live_task.fetch_sub(1, Ordering::SeqCst);
                Ok(Some(input))
            },
            &BatchConfig { concurrency: 3 },
            sink(),
            &job_id,
            CancelFlag::new(),
        )
        .await
        .unwrap();
        assert!(
            peak.load(Ordering::SeqCst) <= 3,
            "peak {} exceeded bound 3",
            peak.load(Ordering::SeqCst)
        );
    }

    #[tokio::test]
    async fn test_batch_empty_rejected() {
        let job_id = JobId::generate();
        let err = run_batch(
            vec![],
            |input: PathBuf| Ok(Some(input)),
            &BatchConfig::default(),
            sink(),
            &job_id,
            CancelFlag::new(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ForgeError::InvalidConfiguration(_)));
    }

    #[test]
    fn test_split_request_per_file() {
        use forge_core::{ConversionOptions, ImageFormat, OutputTarget};
        let request = ConversionRequest {
            inputs: vec![PathBuf::from("a.png"), PathBuf::from("b.png")],
            output_format: ImageFormat::Webp,
            output: OutputTarget::Directory(PathBuf::from("/out")),
            options: ConversionOptions::default(),
        };
        let parts = split_request(&request);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].inputs, vec![PathBuf::from("a.png")]);
    }
}
