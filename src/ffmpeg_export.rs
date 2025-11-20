//! FFmpeg-based video export functionality
//!
//! This module uses the system ffmpeg CLI to export video clips.

use std::path::Path;
use std::process::Command;

/// Get list of supported video file extensions
pub fn get_video_extensions() -> Vec<&'static str> {
    vec![
        "mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", "ts",
    ]
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

/// Get video resolution (width, height) using ffprobe
pub fn get_video_resolution(input_path: &str) -> Result<(u32, u32), String> {
    let output = Command::new("ffprobe")
        .arg("-v")
        .arg("error")
        .arg("-select_streams")
        .arg("v:0")
        .arg("-show_entries")
        .arg("stream=width,height")
        .arg("-of")
        .arg("csv=p=0")
        .arg(input_path)
        .output()
        .map_err(|e| format!("Failed to execute ffprobe: {}", e))?;

    if !output.status.success() {
        return Ok((1920, 1080)); // Default to 1080p if detection fails
    }

    let resolution_str = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // Parse "width,height" format
    if let Some((width_str, height_str)) = resolution_str.split_once(',') {
        let width: u32 = width_str.parse().expect("Failed to parse width");
        let height: u32 = height_str.parse().expect("Failed to parse height");
        return Ok((width, height));
    }

    Ok((1920, 1080))
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
            let mut args = vec![
                "-c:a".to_string(),
                "aac".to_string(),
                "-b:a".to_string(),
                "256k".to_string(),
            ];

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
                args.extend(vec!["-ac".to_string(), "2".to_string()]);
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
/// * `subtitle_settings` - Optional subtitle settings (font, size, bold, italic, color)
/// * `display_subtitles` - Whether to include burned-in subtitles in the output
/// * `subtitle_track` - Optional subtitle track index to burn in
/// * `source_video_width` - Width of the video as displayed in the player (for subtitle scaling)
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(String)` with error message on failure
pub fn export_clip(
    input_path: &str,
    output_path: &str,
    start_secs: f32,
    end_secs: f32,
    subtitle_settings: Option<&crate::SubtitleSettings>,
    display_subtitles: bool,
    subtitle_track: Option<usize>,
    source_video_width: u32,
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

    // Get the output video resolution (which is the same as source for video exports)
    let (output_video_width, _output_video_height) = get_video_resolution(input_path)?;

    // Build subtitle filter if needed
    let subtitle_filter = if display_subtitles && subtitle_track.is_some() {
        if let Some(settings) = subtitle_settings {
            // Subtract 1 because FFmpeg's si parameter is 0-based, but our track indices are 1-based
            let track_idx = subtitle_track.unwrap().saturating_sub(1);

            // Scale font size proportionally to output resolution vs source resolution
            // The subtitle_settings.font_size is calibrated for the player display at source_video_width
            // We need to scale it down/up based on the output resolution
            let scale_factor = output_video_width as f64 / source_video_width as f64;
            let scaled_font_size = (settings.font_size * scale_factor) as i32;

            println!(
                "[export_clip] Subtitle font scaling: source_width={}, output_width={}, scale_factor={:.3}, original_size={:.1}, scaled_size={}",
                source_video_width, output_video_width, scale_factor, settings.font_size, scaled_font_size
            );

            // Convert hex color to FFmpeg format (remove # and convert to BGR format for ASS)
            let color = settings.color.trim_start_matches('#');
            // FFmpeg ASS uses BGR format with &H prefix, so we need to reverse RGB to BGR
            let bgr_color = if color.len() == 6 {
                format!("{}{}{}", &color[4..6], &color[2..4], &color[0..2])
            } else {
                color.to_string()
            };

            // Escape the input path for FFmpeg filter
            // Need to escape: \ ' : [ ] , ;
            let escaped_path = input_path
                .replace("\\", "\\\\")
                .replace("'", "\\'")
                .replace(":", "\\:")
                .replace("[", "\\[")
                .replace("]", "\\]")
                .replace(",", "\\,")
                .replace(";", "\\;");

            Some(format!(
                "subtitles={}:si={}:force_style=FontName={}\\,FontSize={}\\,Bold={}\\,Italic={}\\,PrimaryColour=&H{}",
                escaped_path,
                track_idx,
                settings.font_family,
                scaled_font_size,
                if settings.bold { -1 } else { 0 },
                if settings.italic { -1 } else { 0 },
                bgr_color
            ))
        } else {
            None
        }
    } else {
        None
    };

    // Build ffmpeg command matching atci clipper for maximum speed
    // Key optimization: -ss BEFORE -i for fast seeking
    let mut cmd = Command::new("ffmpeg");

    cmd.arg("-ss").arg(&start_time).arg("-i").arg(input_path);

    // When using subtitles, we need to use copyts and -to instead of -t
    let has_subtitles = subtitle_filter.is_some();

    if has_subtitles {
        cmd.arg("-copyts");
    }

    if is_ts_file {
        // For TS files: use vsync cfr
        if has_subtitles {
            // Use -to with absolute endpoint when subtitles are enabled
            cmd.arg("-to").arg(format!("{}", end_secs));
        } else {
            cmd.arg("-t").arg(&duration_time);
        }

        // Add subtitle filter if present, otherwise just format
        if let Some(ref sub_filter) = subtitle_filter {
            cmd.arg("-vf").arg(format!("{},format=yuv420p", sub_filter));
        } else {
            cmd.arg("-vf").arg("format=yuv420p");
        }

        cmd.arg("-c:v")
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
        // For non-TS files: use double seek and frame count (when no subtitles)
        // or use -to (when subtitles are enabled)
        if has_subtitles {
            // Use -to with absolute endpoint when subtitles are enabled
            cmd.arg("-to")
                .arg(format!("{}", end_secs))
                .arg("-frames:v")
                .arg(frame_count.to_string());
        } else {
            cmd.arg("-ss")
                .arg("00:00:00.001")
                .arg("-t")
                .arg(&duration_time)
                .arg("-frames:v")
                .arg(frame_count.to_string());
        }

        // Add subtitle filter if present
        if let Some(ref sub_filter) = subtitle_filter {
            cmd.arg("-vf").arg(sub_filter);
        }

        cmd.arg("-c:v")
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
        .arg("faststart+frag_keyframe+empty_moov");

    // Don't use avoid_negative_ts when subtitles are enabled
    if !has_subtitles {
        cmd.arg("-avoid_negative_ts").arg("make_zero");
    }

    cmd.arg("-y")
        .arg("-map_chapters")
        .arg("-1")
        .arg(output_path);

    // Debug: print the command
    eprintln!("FFmpeg video export command: {:?}", cmd);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    Ok(())
}

/// Export a video clip as an animated GIF from start_secs to end_secs
///
/// Uses optimized settings from atci clipper:
/// - 10fps for reasonable file size
/// - Scale to 480px width with Lanczos filtering
/// - Palette generation for better colors
/// - Infinite loop
///
/// # Arguments
/// * `input_path` - Path to the input video file
/// * `output_path` - Path where the output GIF should be saved
/// * `start_secs` - Start time in seconds
/// * `end_secs` - End time in seconds
/// * `subtitle_settings` - Optional subtitle settings (font, size, bold, italic, color)
/// * `display_subtitles` - Whether to include subtitles in the GIF
/// * `subtitle_track` - Optional subtitle track index to burn in
/// * `source_video_width` - Width of the video as displayed in the player (for subtitle scaling)
///
/// # Returns
/// * `Ok(())` on success
/// * `Err(String)` with error message on failure
pub fn export_gif(
    input_path: &str,
    output_path: &str,
    start_secs: f32,
    end_secs: f32,
    subtitle_settings: Option<&crate::SubtitleSettings>,
    display_subtitles: bool,
    subtitle_track: Option<usize>,
    source_video_width: u32,
) -> Result<(), String> {
    // Calculate duration
    let duration = end_secs - start_secs;

    // Format timestamps for ffmpeg
    let start_time = format!("{}", start_secs);
    let duration_time = format!("{}", duration);

    // Build the video filter (-vf) for GIF generation
    // Key order from atci clipper: fps=10,scale=480:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse
    let mut filter_parts = Vec::new();

    // GIF output width is 480px (hardcoded in the export)
    let gif_output_width = 480u32;

    // Add subtitle filter if requested and settings provided
    if display_subtitles && subtitle_track.is_some() {
        if let Some(settings) = subtitle_settings {
            // Subtract 1 because FFmpeg's si parameter is 0-based, but our track indices are 1-based
            let track_idx = subtitle_track.unwrap().saturating_sub(1);

            // Scale font size proportionally to output resolution vs source resolution
            // The subtitle_settings.font_size is calibrated for the player display at source_video_width
            // We need to scale it down for the 480px GIF output
            let scale_factor = gif_output_width as f64 / source_video_width as f64;
            let scaled_font_size = (settings.font_size * scale_factor) as i32;

            println!(
                "[export_gif] Subtitle font scaling: source_width={}, gif_output_width={}, scale_factor={:.3}, original_size={:.1}, scaled_size={}",
                source_video_width, gif_output_width, scale_factor, settings.font_size, scaled_font_size
            );

            // Convert hex color to FFmpeg format (remove # and convert to BGR format for ASS)
            let color = settings.color.trim_start_matches('#');
            // FFmpeg ASS uses BGR format with &H prefix, so we need to reverse RGB to BGR
            let bgr_color = if color.len() == 6 {
                format!("{}{}{}", &color[4..6], &color[2..4], &color[0..2])
            } else {
                color.to_string()
            };

            // Build force_style string for subtitle styling
            // Note: FFmpeg subtitles filter uses ASS/SSA style format
            // Escape the input path for FFmpeg filter
            // Need to escape: \ ' : [ ] , ;
            let escaped_path = input_path
                .replace("\\", "\\\\")
                .replace("'", "\\'")
                .replace(":", "\\:")
                .replace("[", "\\[")
                .replace("]", "\\]")
                .replace(",", "\\,")
                .replace(";", "\\;");

            filter_parts.push(format!(
                "subtitles={}:si={}:force_style=FontName={}\\,FontSize={}\\,Bold={}\\,Italic={}\\,PrimaryColour=&H{}",
                escaped_path,
                track_idx,
                settings.font_family,
                scaled_font_size,
                if settings.bold { -1 } else { 0 },
                if settings.italic { -1 } else { 0 },
                bgr_color
            ));
        }
    }

    // Add base filters: fps reduction and scaling
    filter_parts.push("fps=10".to_string());
    filter_parts.push("scale=480:-1:flags=lanczos".to_string());

    // Add palette generation filter
    // split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse
    let vf_filter = format!(
        "{},split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse",
        filter_parts.join(",")
    );

    // Build ffmpeg command with correct argument order from atci clipper:
    // -ss {start} -t {duration} -i {input} -vf {filter} -loop 0 -y {output}
    // Note: Unlike video exports, GIFs don't need -to for subtitles
    let mut cmd = Command::new("ffmpeg");

    cmd.arg("-ss")
        .arg(&start_time)
        .arg("-t")
        .arg(&duration_time)
        .arg("-i")
        .arg(input_path)
        .arg("-copyts");

    cmd.arg("-vf")
        .arg(&vf_filter)
        .arg("-loop")
        .arg("0") // Infinite loop
        .arg("-y") // Overwrite output file
        .arg(output_path);

    // Debug: print the command and filter
    eprintln!("FFmpeg GIF export command: {:?}", cmd);
    eprintln!("GIF filter chain: {}", vf_filter);

    let output = cmd
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    Ok(())
}
