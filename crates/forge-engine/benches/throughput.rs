//! Throughput benches (spec §36 / Master §28): measure first, then tune.
//!
//! - `single_png_to_webp`: one 512×512 image, lossy WebP q80.
//! - `batch_10_png_to_webp`: 10 images through the bounded pool (4 workers).
//!
//! Run: `cargo bench -p forge-engine`. Baselines land in
//! `target/criterion`; compare across worker counts with
//! `-- --quick` for smoke runs.

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use std::hint::black_box;

use forge_core::{CanonicalImage, ColorSpace, ImageDimensions, PixelFormat};

/// Deterministic 512×512 RGBA gradient (no fixtures, no I/O).
fn gradient_512() -> CanonicalImage {
    let dimensions = ImageDimensions::new(512, 512).expect("non-zero");
    let mut pixels = Vec::with_capacity(512 * 512 * 4);
    for y in 0..512 {
        for x in 0..512 {
            pixels.extend_from_slice(&[
                (x % 256) as u8,
                (y % 256) as u8,
                ((x + y) % 256) as u8,
                255,
            ]);
        }
    }
    CanonicalImage {
        dimensions,
        pixel_format: PixelFormat::Rgba8,
        color_space: ColorSpace::Srgb,
        pixels,
        has_alpha: false,
    }
}

fn bench_single_png_to_webp(criterion: &mut Criterion) {
    use forge_core::ConversionOptions;
    use forge_core::{ImageDecoder as _, ImageEncoder as _};
    use forge_image::{ForgeImageDecoder, ForgeImageEncoder};
    // Encode once to get realistic PNG input bytes.
    let encoder = ForgeImageEncoder;
    let image = gradient_512();
    let png = encoder
        .encode(
            &image,
            forge_core::ImageFormat::Png,
            &ConversionOptions::default(),
        )
        .expect("fixture encodes");
    let decoder = ForgeImageDecoder;
    let options = ConversionOptions {
        quality: 80,
        ..Default::default()
    };

    let mut group = criterion.benchmark_group("single");
    group.throughput(Throughput::Bytes(png.len() as u64));
    group.bench_function("png_to_webp_q80", |bencher| {
        bencher.iter(|| {
            let image = decoder
                .decode(black_box(&png), Some(forge_core::ImageFormat::Png))
                .expect("decodes");
            let out = encoder
                .encode(black_box(&image), forge_core::ImageFormat::Webp, &options)
                .expect("encodes");
            black_box(out);
        });
    });
    group.finish();
}

fn bench_batch_10_png_to_webp(criterion: &mut Criterion) {
    use forge_core::{ConversionOptions, ImageEncoder as _, ImageFormat};
    use forge_engine::{
        BatchConfig, CancelFlag, EngineDeps, NullSink, Orchestrator, StdFileSystem,
    };
    use forge_image::{ForgeImageDecoder, ForgeImageEncoder, ResizeStep};
    use std::sync::Arc;
    // 10 PNG files in a temp dir (setup outside the timed loop).
    let dir = std::env::temp_dir().join("forgeconvert-bench-batch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("bench dir");
    let encoder = ForgeImageEncoder;
    let png = encoder
        .encode(
            &gradient_512(),
            ImageFormat::Png,
            &ConversionOptions::default(),
        )
        .expect("fixture encodes");
    let inputs: Vec<std::path::PathBuf> = (0..10)
        .map(|i| {
            let path = dir.join(format!("bench-{i}.png"));
            std::fs::write(&path, &png).expect("bench input");
            path
        })
        .collect();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let mut group = criterion.benchmark_group("batch");
    group.sample_size(10);
    group.bench_function("10_png_to_webp_4_workers", |bencher| {
        bencher.iter(|| {
            runtime.block_on(async {
                let out_dir = dir.join("out");
                let _ = std::fs::create_dir_all(&out_dir);
                let config = BatchConfig { concurrency: 4 };
                let sink: Arc<dyn forge_core::JobEventSink> = Arc::new(NullSink);
                let job_id = forge_core::JobId::generate();
                let report = forge_engine::run_batch(
                    black_box(inputs.clone()),
                    move |input: std::path::PathBuf| {
                        let fs = StdFileSystem;
                        let decoder = ForgeImageDecoder;
                        let encoder = ForgeImageEncoder;
                        let fit = ResizeStep;
                        let transforms: [&dyn forge_core::TransformStep; 1] = [&fit];
                        let engine = Orchestrator::new(EngineDeps {
                            decoder: &decoder,
                            encoder: &encoder,
                            transforms: &transforms,
                            fs: &fs,
                        });
                        let options = ConversionOptions {
                            quality: 80,
                            ..Default::default()
                        }
                        .validated()
                        .expect("valid");
                        let request = forge_core::ConversionRequest {
                            inputs: vec![input],
                            output_format: ImageFormat::Webp,
                            output: forge_core::OutputTarget::Directory(out_dir.clone()),
                            options,
                        }
                        .validated()
                        .expect("valid");
                        let result = engine
                            .run(request, &NullSink, &forge_core::NeverCancel)
                            .expect("converts");
                        Ok(result.outputs.into_iter().next())
                    },
                    &config,
                    sink,
                    &job_id,
                    CancelFlag::new(),
                )
                .await
                .expect("batch");
                black_box(report);
            });
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_single_png_to_webp,
    bench_batch_10_png_to_webp
);
criterion_main!(benches);
