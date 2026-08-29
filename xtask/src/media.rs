use std::path::{Path, PathBuf};

use color_quant::NeuQuant;

use crate::error::{TaskError, TaskResult};
use crate::workspace::{node_modules_binary, run_checked, workspace_root};

pub const GIF_WIDTH: u16 = 720;
pub const GIF_FRAME_STRIDE: usize = 2;
pub const GIF_FRAME_DELAY_CENTISECONDS: u16 = 22;

pub fn media_directory() -> PathBuf {
    workspace_root().join("docs").join("media")
}

pub fn frames_directory() -> PathBuf {
    media_directory().join("frames")
}

pub fn run(validate_only: bool) -> TaskResult<()> {
    if !validate_only {
        derive_animation()?;
        derive_mp4()?;
    }
    validate()
}

pub fn derive_animation() -> TaskResult<()> {
    let all = collect_frames()?;
    let frames: Vec<std::path::PathBuf> = all.into_iter().step_by(GIF_FRAME_STRIDE).collect();
    if frames.is_empty() {
        return Err(TaskError::Missing(format!(
            "no captured frames were found in {}",
            frames_directory().display()
        )));
    }
    let target = media_directory().join("demo.gif");
    encode_gif(&frames, &target)?;
    println!(
        "wrote {} from {} captured frames",
        target.display(),
        frames.len()
    );
    Ok(())
}

pub fn derive_mp4() -> TaskResult<()> {
    let source = media_directory().join("demo.webm");
    if !source.is_file() {
        return Err(TaskError::Missing(format!(
            "the recorded video is missing at {}",
            source.display()
        )));
    }
    let target = media_directory().join("demo.mp4");
    let ffmpeg = locate_ffmpeg()?;
    run_checked(
        &ffmpeg.display().to_string(),
        &[
            "-y",
            "-loglevel",
            "error",
            "-i",
            &source.display().to_string(),
            "-c:v",
            "libx264",
            "-preset",
            "slow",
            "-crf",
            "23",
            "-pix_fmt",
            "yuv420p",
            "-movflags",
            "+faststart",
            "-an",
            &target.display().to_string(),
        ],
        &workspace_root(),
        &[],
    )?;
    println!("wrote {}", target.display());
    Ok(())
}

fn locate_ffmpeg() -> TaskResult<PathBuf> {
    if let Ok(configured) = std::env::var("HEIKAS_FFMPEG") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Ok(path);
        }
    }
    let candidates = [
        ".pnpm/ffmpeg-static@5.2.0_supports-color@7.2.0/node_modules/ffmpeg-static/ffmpeg",
        "ffmpeg-static/ffmpeg",
    ];
    for candidate in candidates {
        if let Some(path) = node_modules_binary(candidate) {
            return Ok(path);
        }
    }
    if let Some(path) = search_path("ffmpeg") {
        return Ok(path);
    }
    Err(TaskError::Missing(
        "no ffmpeg executable was found. Install it or run `pnpm install` so the local copy is fetched."
            .to_string(),
    ))
}

fn search_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

fn collect_frames() -> TaskResult<Vec<PathBuf>> {
    let directory = frames_directory();
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut frames: Vec<PathBuf> = std::fs::read_dir(&directory)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("png"))
        .collect();
    frames.sort();
    Ok(frames)
}

struct DecodedFrame {
    width: u16,
    height: u16,
    pixels: Vec<u8>,
}

fn decode_png(path: &Path) -> TaskResult<DecodedFrame> {
    let file = std::fs::File::open(path)?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|error| TaskError::Encoding(format!("{}: {error}", path.display())))?;
    let mut buffer = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|error| TaskError::Encoding(format!("{}: {error}", path.display())))?;
    let channels = match info.color_type {
        png::ColorType::Rgba => 4,
        png::ColorType::Rgb => 3,
        other => {
            return Err(TaskError::Encoding(format!(
                "{} uses the unsupported colour type {other:?}",
                path.display()
            )))
        }
    };
    let mut pixels = Vec::with_capacity((info.width * info.height * 4) as usize);
    for chunk in buffer[..info.buffer_size()].chunks_exact(channels) {
        pixels.push(chunk[0]);
        pixels.push(chunk[1]);
        pixels.push(chunk[2]);
        pixels.push(if channels == 4 { chunk[3] } else { 255 });
    }
    Ok(DecodedFrame {
        width: info.width as u16,
        height: info.height as u16,
        pixels,
    })
}

fn downscale(frame: &DecodedFrame, target_width: u16) -> DecodedFrame {
    if frame.width <= target_width {
        return DecodedFrame {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels.clone(),
        };
    }
    let ratio = f64::from(frame.width) / f64::from(target_width);
    let target_height = ((f64::from(frame.height) / ratio).round() as u16).max(1);
    let mut pixels = Vec::with_capacity(usize::from(target_width) * usize::from(target_height) * 4);
    let block = ratio.ceil() as u32;
    for row in 0..target_height {
        for column in 0..target_width {
            let source_x = (f64::from(column) * ratio) as u32;
            let source_y = (f64::from(row) * ratio) as u32;
            let mut totals = [0u32; 4];
            let mut samples = 0u32;
            for offset_y in 0..block {
                for offset_x in 0..block {
                    let x = source_x + offset_x;
                    let y = source_y + offset_y;
                    if x >= u32::from(frame.width) || y >= u32::from(frame.height) {
                        continue;
                    }
                    let index = ((y * u32::from(frame.width) + x) * 4) as usize;
                    for (channel, total) in totals.iter_mut().enumerate() {
                        *total += u32::from(frame.pixels[index + channel]);
                    }
                    samples += 1;
                }
            }
            let divisor = samples.max(1);
            for total in totals {
                pixels.push((total / divisor) as u8);
            }
        }
    }
    DecodedFrame {
        width: target_width,
        height: target_height,
        pixels,
    }
}

fn encode_gif(frames: &[PathBuf], target: &Path) -> TaskResult<()> {
    let first = downscale(&decode_png(&frames[0])?, GIF_WIDTH);
    let mut palette_source: Vec<u8> = Vec::new();
    for path in frames.iter().step_by((frames.len() / 12).max(1)) {
        let frame = downscale(&decode_png(path)?, GIF_WIDTH);
        palette_source.extend_from_slice(&frame.pixels);
    }
    let quantiser = NeuQuant::new(10, 256, &palette_source);
    let palette: Vec<u8> = quantiser.color_map_rgb();

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(target)?;
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder = gif::Encoder::new(&mut writer, first.width, first.height, &palette)
        .map_err(|error| TaskError::Encoding(error.to_string()))?;
    encoder
        .set_repeat(gif::Repeat::Infinite)
        .map_err(|error| TaskError::Encoding(error.to_string()))?;

    for path in frames {
        let frame = downscale(&decode_png(path)?, GIF_WIDTH);
        if frame.width != first.width || frame.height != first.height {
            return Err(TaskError::Encoding(format!(
                "{} has a different size from the first captured frame",
                path.display()
            )));
        }
        let indices: Vec<u8> = frame
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| quantiser.index_of(pixel) as u8)
            .collect();
        let mut gif_frame =
            gif::Frame::from_indexed_pixels(frame.width, frame.height, indices, None);
        gif_frame.delay = GIF_FRAME_DELAY_CENTISECONDS;
        encoder
            .write_frame(&gif_frame)
            .map_err(|error| TaskError::Encoding(error.to_string()))?;
    }
    Ok(())
}

pub fn validate() -> TaskResult<()> {
    let root = workspace_root();
    let repository = heikas_policy::TrackedRepository::discover(&root)
        .map_err(|error| TaskError::Invalid(error.to_string()))?;
    let findings = heikas_policy::rules::documentation::check(&repository)
        .map_err(|error| TaskError::Invalid(error.to_string()))?;
    let media_findings: Vec<_> = findings
        .into_iter()
        .filter(|finding| finding.rule == heikas_policy::rules::documentation::MEDIA_RULE)
        .collect();
    if media_findings.is_empty() {
        println!("Every README media reference resolves to real captured media.");
        return Ok(());
    }
    for finding in &media_findings {
        eprintln!("{}", finding.message);
        eprintln!("  {}", finding.remedy);
    }
    Err(TaskError::StepFailed {
        step: "README media validation".to_string(),
    })
}
