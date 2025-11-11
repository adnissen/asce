//! FFmpeg-based video export functionality
//!
//! This module uses the system ffmpeg CLI to export video clips.

use std::path::Path;
use std::process::Command;

/// Get list of supported video file extensions
pub fn get_video_extensions() -> Vec<&'static str> {
    vec!["mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", "ts"]
}

/// Get video framerate using ffprobe
fn get_video_fps(input_path: &str) -> Result<f32, String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=r_frame_rate")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(input_path)
        .output()
        .map_err(|e| format!("Failed to execute ffprobe: {}", e))?;

    if !output.status.success() {
        return Ok(30.0); // Default to 30fps if detection fails
    }

    let fps_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Parse fraction format (e.g., "30/1" or "2997/100")
    if let Some((num_str, den_str)) = fps_str.split_once('/') {
        let numerator: f32 = num_str.parse().unwrap_or(30.0);
        let denominator: f32 = den_str.parse().unwrap_or(1.0);
        if denominator > 0.0 {
            return Ok(numerator / denominator);
        }
    }

    // Try parsing as simple float
    fps_str.parse::<f32>().or(Ok(30.0))
}

/// Check if file needs advanced audio re-encoding based on channel layout
fn check_if_advanced_audio_reencoding_needed(input_path: &str) -> Result<Option<String>, String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("a:0")
        .arg("-show_entries")
        .arg("stream=channel_layout")
        .arg("-of")
        .arg("default=noprint_wrappers=1:nokey=1")
        .arg(input_path)
        .output()
        .map_err(|e| format!("Failed to execute ffprobe: {}", e))?;

    if !output.status.success() {
        return Ok(None);
    }

    let layout = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if layout.is_empty() || layout == "stereo" || layout == "mono" {
        Ok(None)
    } else {
        Ok(Some(layout))
    }
}

/// Get audio codec arguments based on file type and audio characteristics
fn get_audio_codec_args(input_path: &str) -> Result<Vec<String>, String> {
    let path = Path::new(input_path);
    let extension = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Check if file type needs re-encoding
    let needs_reencoding = matches!(extension.as_str(), "mkv" | "webm" | "avi" | "mov");

    // For TS files, always use specific audio encoding
    if extension == "ts" {
        return Ok(vec![
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "256k".to_string(),
            "-ar".to_string(),
            "48000".to_string(),
            "-ac".to_string(),
            "2".to_string(),
        ]);
    }

    if needs_reencoding {
        // Check for advanced audio that needs special handling
        if let Ok(Some(layout)) = check_if_advanced_audio_reencoding_needed(input_path) {
            let mut args = vec!["-c:a".to_string(), "aac".to_string(), "-b:a".to_string(), "256k".to_string()];

            // Handle different channel layouts
            if layout == "5.1" {
                // Map 5.1 channels properly
                args.extend(vec![
                    "-af".to_string(),
                    "channelmap=channel_layout=5.1".to_string(),
                ]);
            } else if layout == "5.1(side)" {
                // Convert side channels to back channels
                args.extend(vec![
                    "-af".to_string(),
                    "channelmap=channel_layout=5.1".to_string(),
                ]);
            } else if layout.starts_with("7.1") {
                // Keep 7.1 as is
                args.extend(vec![
                    "-af".to_string(),
                    "channelmap=channel_layout=7.1".to_string(),
                ]);
            } else {
                // Downmix to stereo for other layouts
                args.extend(vec![
                    "-ac".to_string(),
                    "2".to_string(),
                ]);
            }

            return Ok(args);
        }

        // Standard AAC re-encoding
        return Ok(vec![
            "-c:a".to_string(),
            "aac".to_string(),
            "-b:a".to_string(),
            "256k".to_string(),
        ]);
    }

    // Copy audio without re-encoding
    Ok(vec!["-c:a".to_string(), "copy".to_string()])
}

/// Export a video clip from start_secs to end_secs using ffmpeg CLI
///
/// # Arguments
/// * `input_path` - Path to the input video file
/// * `output_path` - Path where the output clip should be saved
/// * `start_secs` - Start time in seconds
/// * `end_secs` - End time in seconds
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(String)` with error message on failure
pub fn export_clip(
    input_path: &str,
    output_path: &str,
    start_secs: f32,
    end_secs: f32,
) -> Result<(), String> {
    // Calculate duration
    let duration = end_secs - start_secs;

    // Format timestamps for ffmpeg
    let start_time = format!("{}", start_secs);
    let duration_time = format!("{}", duration);

    // Detect video metadata using ffprobe
    let fps = get_video_fps(input_path).unwrap_or(30.0);
    let frame_count = (duration * fps).trunc() as u32;

    // Get audio codec arguments based on file analysis
    let audio_args = get_audio_codec_args(input_path)?;

    // Check if input is a .ts file for special handling
    let is_ts_file = input_path.ends_with(".ts");

    // Build ffmpeg command matching atci clipper for maximum speed
    // Key optimization: -ss BEFORE -i for fast seeking
    let mut cmd = Command::new("ffmpeg");

    cmd.arg("-ss")
        .arg(&start_time)
        .arg("-i")
        .arg(input_path);

    if is_ts_file {
        // For TS files: use vsync cfr
        cmd.arg("-t")
            .arg(&duration_time)
            .arg("-vf")
            .arg("format=yuv420p")
            .arg("-c:v")
            .arg("libx264")
            .arg("-profile:v")
            .arg("baseline")
            .arg("-level")
            .arg("3.1")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-vsync")
            .arg("cfr");
    } else {
        // For non-TS files: use double seek and frame count
        cmd.arg("-ss")
            .arg("00:00:00.001")
            .arg("-t")
            .arg(&duration_time)
            .arg("-frames:v")
            .arg(frame_count.to_string())
            .arg("-c:v")
            .arg("libx264")
            .arg("-profile:v")
            .arg("baseline")
            .arg("-level")
            .arg("3.1")
            .arg("-pix_fmt")
            .arg("yuv420p");
    }

    // Add audio codec arguments (detected based on source file)
    for arg in audio_args {
        cmd.arg(arg);
    }

    // Quality and optimization flags
    cmd.arg("-crf")
        .arg("28")
        .arg("-preset")
        .arg("ultrafast")
        .arg("-movflags")
        .arg("faststart+frag_keyframe+empty_moov")
        .arg("-avoid_negative_ts")
        .arg("make_zero")
        .arg("-y")
        .arg("-map_chapters")
        .arg("-1")
        .arg(output_path);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    Ok(())
}
