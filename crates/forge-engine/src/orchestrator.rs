//! Single-file pipeline (spec §11):
//! validate → detect → decode → normalize → transform → encode →
//! temp write → flush/sync → validate → atomic rename → Completed.
//!
//! Cancellation is checked between stages; collisions follow
//! [`CollisionPolicy`]; progress flows to the [`JobEventSink`].

use std::time::{Duration, Instant};

use forge_core::{
    CancelToken, ConversionJob, ConversionOptions, ConversionRequest, ConversionResult,
    FileSystem as _, ForgeError, ImageDecoder, ImageEncoder, JobEventSink, JobProgress, JobStatus,
    OutputTarget, Result, TransformStep,
};

use crate::filesystem::StdFileSystem;
use crate::naming::resolve_output;

/// Injected collaborators (hexagonal wiring — no concrete adapters here).
pub struct EngineDeps<'a> {
    pub decoder: &'a dyn ImageDecoder,
    pub encoder: &'a dyn ImageEncoder,
    pub transforms: &'a [&'a dyn TransformStep],
    pub fs: &'a StdFileSystem,
}

/// Job-level orchestrator: one [`ConversionRequest`] → [`ConversionResult`].
pub struct Orchestrator<'a> {
    deps: EngineDeps<'a>,
}

impl<'a> Orchestrator<'a> {
    /// Wire decoder/encoder/transforms/filesystem (all behind ports).
    #[must_use]
    pub fn new(deps: EngineDeps<'a>) -> Self {
        Self { deps }
    }

    /// Run every input in `request` sequentially (batch parallelism
    /// lives in `batch.rs`; this stays single-file and cancellable).
    /// Skips (collision policy) land in `result.skipped`, never failures.
    pub fn run(
        &self,
        request: ConversionRequest,
        sink: &dyn JobEventSink,
        cancel: &dyn CancelToken,
    ) -> Result<ConversionResult> {
        let request = request.validated()?;
        let started = Instant::now();
        let mut job = ConversionJob::new(request.clone());
        job.advance(JobStatus::Running)?;
        sink.progress(
            &job.id,
            &JobProgress::new(0.0, Some("starting".to_string())),
        );

        let mut outputs = Vec::new();
        let mut failures = Vec::new();
        let mut skipped = Vec::new();
        let total = request.inputs.len();

        for (index, input) in request.inputs.iter().enumerate() {
            cancel.check()?;
            let fraction = index as f32 / total.max(1) as f32;
            sink.progress(
                &job.id,
                &JobProgress::new(fraction, Some(format!("converting {}", input.display()))),
            );
            match self.convert_one(&request, input, &request.options, cancel) {
                Ok(SingleOutcome::Written(path)) => outputs.push(path),
                Ok(SingleOutcome::Skipped(path)) => skipped.push(path),
                Err(ForgeError::Cancelled) => {
                    job.advance(JobStatus::Cancelled)?;
                    sink.failed(&job.id, &ForgeError::Cancelled);
                    return Err(ForgeError::Cancelled);
                }
                Err(e) => failures.push(format!("{}: {e}", input.display())),
            }
        }

        cancel.check()?;
        let result = ConversionResult {
            outputs,
            failures,
            skipped,
            duration: started.elapsed(),
        };
        job.advance(JobStatus::Completed)?;
        job.progress = JobProgress::new(1.0, Some("done".to_string()));
        job.result = Some(result.clone());
        sink.finished(&job.id, job.result.as_ref().expect("just set"));
        Ok(result)
    }

    /// The §11 pipeline for one file. `Skipped` out: collision policy said Skip.
    fn convert_one(
        &self,
        request: &ConversionRequest,
        input: &std::path::Path,
        options: &ConversionOptions,
        cancel: &dyn CancelToken,
    ) -> Result<SingleOutcome> {
        cancel.check()?;
        // `StdFileSystem::read` already returns structured variants
        // (NotFound → InvalidFile, denied → PermissionDenied, …).
        let bytes = self.deps.fs.read(input)?;

        cancel.check()?;
        let format = forge_core::detect_input_format(input, Some(&bytes))?;
        if format == request.output_format {
            return Err(ForgeError::InvalidConfiguration(format!(
                "input {} is already {}",
                input.display(),
                format.mime_type()
            )));
        }

        cancel.check()?;
        let mut image = self.deps.decoder.decode(&bytes, Some(format))?;

        cancel.check()?;
        for step in self.deps.transforms {
            image = step.apply(image, options)?;
        }

        cancel.check()?;
        let encoded = self
            .deps
            .encoder
            .encode(&image, request.output_format, options)?;

        cancel.check()?;
        // Both output kinds funnel through skip-aware resolution:
        // Skip → record, never write; Fail → OutputExists error.
        let output_path = match &request.output {
            OutputTarget::File(path) => {
                match crate::naming::apply_collision(path, options.on_collision)? {
                    Some(path) => path,
                    // Skip: echo the EXISTING path so scripts keep a usable path.
                    None => return Ok(SingleOutcome::Skipped(path.clone())),
                }
            }
            OutputTarget::Directory(dir) => {
                match crate::naming::existing_candidate(input, request.output_format, Some(dir)) {
                    Some(existing) if existing.exists() => {
                        match resolve_output(
                            input,
                            request.output_format,
                            Some(dir),
                            options.on_collision,
                        )? {
                            Some(path) => path,
                            // Skip: echo the EXISTING path, not the input.
                            None => return Ok(SingleOutcome::Skipped(existing)),
                        }
                    }
                    _ => resolve_output(
                        input,
                        request.output_format,
                        Some(dir),
                        options.on_collision,
                    )?
                    .expect("fresh candidate cannot be Skip"),
                }
            }
        };

        self.deps.fs.write_atomic(&output_path, &encoded)?;
        Ok(SingleOutcome::Written(output_path))
    }

    /// How long the last `run` phase took (helper for history, Phase 9).
    #[must_use]
    pub fn elapsed_since(start: Instant) -> Duration {
        start.elapsed()
    }
}

/// One file's outcome: written output or policy skip.
#[derive(Debug)]
enum SingleOutcome {
    Written(std::path::PathBuf),
    Skipped(std::path::PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;
    use forge_core::{
        BackgroundPolicy, CollisionPolicy, ImageDimensions, MetadataPolicy, NeverCancel,
    };
    use std::sync::Mutex;

    /// In-memory decoder producing a 2×2 RGBA gradient.
    struct TestDecoder;
    impl ImageDecoder for TestDecoder {
        fn supported_inputs(&self) -> &'static [forge_core::ImageFormat] {
            &[]
        }
        fn decode(
            &self,
            _bytes: &[u8],
            _hint: Option<forge_core::ImageFormat>,
        ) -> Result<forge_core::CanonicalImage> {
            Ok(forge_core::CanonicalImage {
                dimensions: ImageDimensions::new(2, 2).unwrap(),
                pixel_format: forge_core::PixelFormat::Rgba8,
                color_space: forge_core::ColorSpace::Srgb,
                pixels: vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 200,
                ],
                has_alpha: true,
            })
        }
    }

    /// Encoder capturing calls; emits PNG magic + dimensions.
    struct TestEncoder {
        calls: Mutex<Vec<forge_core::ImageFormat>>,
    }
    impl ImageEncoder for TestEncoder {
        fn supported_outputs(&self) -> &'static [forge_core::ImageFormat] {
            &[]
        }
        fn encode(
            &self,
            image: &forge_core::CanonicalImage,
            target: forge_core::ImageFormat,
            _options: &ConversionOptions,
        ) -> Result<Vec<u8>> {
            self.calls.lock().expect("lock").push(target);
            // Minimal PNG: real magic + IHDR dims so decodability-style
            // assertions stay meaningful without a codec.
            let (w, h) = (image.dimensions.width, image.dimensions.height);
            let mut bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            bytes.extend_from_slice(&w.to_be_bytes());
            bytes.extend_from_slice(&h.to_be_bytes());
            Ok(bytes)
        }
    }

    struct TestSink {
        events: Mutex<Vec<String>>,
    }
    impl JobEventSink for TestSink {
        fn progress(&self, _id: &forge_core::JobId, progress: &JobProgress) {
            self.events
                .lock()
                .expect("lock")
                .push(format!("progress:{:.2}", progress.fraction));
        }
        fn finished(&self, _id: &forge_core::JobId, _result: &ConversionResult) {
            self.events
                .lock()
                .expect("lock")
                .push("finished".to_string());
        }
        fn failed(&self, _id: &forge_core::JobId, _error: &ForgeError) {
            self.events.lock().expect("lock").push("failed".to_string());
        }
    }

    struct CancelAfter;
    impl CancelToken for CancelAfter {
        fn is_cancelled(&self) -> bool {
            true
        }
    }

    /// Scratch dir under the OS temp area (auto-removed).
    struct Scratch {
        dir: std::path::PathBuf,
    }
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("forgeconvert-test-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch");
            Self { dir }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn test_request(dir: &std::path::Path, name: &str) -> (ConversionRequest, std::path::PathBuf) {
        let input = dir.join(name);
        std::fs::write(&input, b"\x89PNG-fake-bytes").expect("input");
        let request = ConversionRequest {
            inputs: vec![input.clone()],
            output_format: forge_core::ImageFormat::Webp,
            output: OutputTarget::Directory(dir.to_path_buf()),
            options: ConversionOptions {
                quality: 80,
                max_dimensions: None,
                metadata: MetadataPolicy::Preserve,
                background: BackgroundPolicy::default(),
                on_collision: CollisionPolicy::RenameAuto,
            },
        }
        .validated()
        .unwrap();
        (request, input)
    }

    #[test]
    fn test_run_writes_output_and_reports_finished() {
        let scratch = Scratch::new("orchestrator-ok");
        let (request, _input) = test_request(&scratch.dir, "logo.png");
        let decoder = TestDecoder;
        let encoder = TestEncoder {
            calls: Mutex::new(Vec::new()),
        };
        let fs = StdFileSystem;
        let engine = Orchestrator::new(EngineDeps {
            decoder: &decoder,
            encoder: &encoder,
            transforms: &[],
            fs: &fs,
        });
        let sink = TestSink {
            events: Mutex::new(Vec::new()),
        };
        let result = engine.run(request, &sink, &NeverCancel).unwrap();
        assert_eq!(result.outputs.len(), 1);
        assert!(result.failures.is_empty());
        assert!(result.outputs[0].exists(), "atomic write landed");
        let events = sink.events.lock().expect("lock");
        assert!(events.contains(&"finished".to_string()));
        assert_eq!(encoder.calls.lock().expect("lock").len(), 1);
    }

    #[test]
    fn test_same_format_rejected_without_encoding() {
        let scratch = Scratch::new("orchestrator-same");
        let (mut request, _) = test_request(&scratch.dir, "logo.webp");
        request.output_format = forge_core::ImageFormat::Png; // decoded hint is PNG bytes…
                                                              // …but force the sniff to see PNG while target is PNG:
        request.output_format = forge_core::ImageFormat::Png;
        // Rewrite input with PNG magic so detect → Png == target Png.
        std::fs::write(
            &request.inputs[0],
            [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A],
        )
        .expect("input");
        let decoder = TestDecoder;
        let encoder = TestEncoder {
            calls: Mutex::new(Vec::new()),
        };
        let fs = StdFileSystem;
        let engine = Orchestrator::new(EngineDeps {
            decoder: &decoder,
            encoder: &encoder,
            transforms: &[],
            fs: &fs,
        });
        let sink = TestSink {
            events: Mutex::new(Vec::new()),
        };
        let result = engine.run(request, &sink, &NeverCancel).unwrap();
        assert!(
            !result.failures.is_empty(),
            "same-format input recorded as failure, not encoded"
        );
        assert!(encoder.calls.lock().expect("lock").is_empty());
    }

    #[test]
    fn test_cancellation_before_first_file() {
        let scratch = Scratch::new("orchestrator-cancel");
        let (request, _) = test_request(&scratch.dir, "logo.png");
        let decoder = TestDecoder;
        let encoder = TestEncoder {
            calls: Mutex::new(Vec::new()),
        };
        let fs = StdFileSystem;
        let engine = Orchestrator::new(EngineDeps {
            decoder: &decoder,
            encoder: &encoder,
            transforms: &[],
            fs: &fs,
        });
        let sink = TestSink {
            events: Mutex::new(Vec::new()),
        };
        let err = engine.run(request, &sink, &CancelAfter).unwrap_err();
        assert!(matches!(err, ForgeError::Cancelled));
        assert!(encoder.calls.lock().expect("lock").is_empty());
    }

    #[test]
    fn test_missing_input_recorded_not_panicked() {
        let scratch = Scratch::new("orchestrator-missing");
        let request = ConversionRequest {
            inputs: vec![scratch.dir.join("ghost.png")],
            output_format: forge_core::ImageFormat::Webp,
            output: OutputTarget::Directory(scratch.dir.clone()),
            options: ConversionOptions::default(),
        }
        .validated()
        .unwrap();
        let decoder = TestDecoder;
        let encoder = TestEncoder {
            calls: Mutex::new(Vec::new()),
        };
        let fs = StdFileSystem;
        let engine = Orchestrator::new(EngineDeps {
            decoder: &decoder,
            encoder: &encoder,
            transforms: &[],
            fs: &fs,
        });
        let sink = TestSink {
            events: Mutex::new(Vec::new()),
        };
        let result = engine.run(request, &sink, &NeverCancel).unwrap();
        assert!(result.outputs.is_empty());
        assert_eq!(result.failures.len(), 1);
    }

    #[test]
    fn test_skip_policy_records_skip_not_failure() {
        let scratch = Scratch::new("orchestrator-skip");
        let (mut request, _input) = test_request(&scratch.dir, "logo.png");
        // Pre-create the output so Skip triggers.
        let existing = scratch.dir.join("logo.webp");
        std::fs::write(&existing, b"taken").expect("seed");
        request.options.on_collision = CollisionPolicy::Skip;
        let decoder = TestDecoder;
        let encoder = TestEncoder {
            calls: Mutex::new(Vec::new()),
        };
        let fs = StdFileSystem;
        let engine = Orchestrator::new(EngineDeps {
            decoder: &decoder,
            encoder: &encoder,
            transforms: &[],
            fs: &fs,
        });
        let sink = TestSink {
            events: Mutex::new(Vec::new()),
        };
        let result = engine.run(request, &sink, &NeverCancel).unwrap();
        assert!(result.outputs.is_empty());
        assert!(result.failures.is_empty(), "skip is not a failure");
        // Skip echoes the EXISTING output path (usable by scripts), not the input.
        assert_eq!(result.skipped, vec![existing.clone()]);
        // Seeded file untouched.
        assert_eq!(std::fs::read(&existing).unwrap(), b"taken");
    }
}
