//! Subtitle stream detection using ffprobe
//!
//! This module provides functionality to detect and enumerate subtitle streams
//! in video files that can be exported as SRT (SubRip) format.

use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;

/// Information about a subtitle stream found in a video file
#[derive(Debug, Clone)]
pub struct SubtitleStream {
    /// Stream index (e.g., 0, 1, 2...)
    pub index: usize,
    /// Codec name (e.g., "subrip", "ass", "webvtt", "mov_text")
    pub codec_name: String,
    /// Language code if available (e.g., "eng", "spa", "fra")
    pub language: Option<String>,
    /// Human-readable display title for UI
    pub display_title: String,
}

impl SubtitleStream {
    /// Check if this subtitle stream can be exported as SRT
    pub fn is_text_based(&self) -> bool {
        matches!(
            self.codec_name.as_str(),
            "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "srt" | "text"
        )
    }
}

/// Detect all text-based subtitle streams in a video file
///
/// Uses ffprobe to enumerate subtitle streams and filters to only include
/// streams that can be exported as SRT format.
///
/// # Arguments
///
/// * `file_path` - Path to the video file
///
/// # Returns
///
/// A vector of `SubtitleStream` structs, one for each text-based subtitle stream found.
/// Returns an empty vector if no suitable streams are found or if ffprobe fails.
pub fn detect_subtitle_streams(file_path: &str) -> Vec<SubtitleStream> {
    // Use ffprobe to get all subtitle streams in JSON format
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "s", // Select subtitle streams
            "-show_entries",
            "stream=index,codec_name:stream_tags=language", // Get index, codec, and language
            "-of",
            "json", // Output as JSON for easier parsing
            file_path,
        ])
        .output();

    let output = match output {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Failed to execute ffprobe: {}", e);
            return Vec::new();
        }
    };

    if !output.status.success() {
        eprintln!("ffprobe failed: {}", String::from_utf8_lossy(&output.stderr));
        return Vec::new();
    }

    let json_output = String::from_utf8_lossy(&output.stdout);

    // Parse JSON output
    parse_ffprobe_json(&json_output)
}

/// ffprobe JSON output structures
#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: usize,
    codec_name: String,
    #[serde(default)]
    tags: HashMap<String, String>,
}

/// Parse ffprobe JSON output to extract subtitle stream information
fn parse_ffprobe_json(json: &str) -> Vec<SubtitleStream> {
    let mut streams = Vec::new();

    // Parse JSON using serde_json
    let ffprobe_output: FfprobeOutput = match serde_json::from_str(json) {
        Ok(output) => output,
        Err(e) => {
            eprintln!("Failed to parse ffprobe JSON: {}", e);
            return streams;
        }
    };

    let mut subtitle_index = 0;
    for stream in ffprobe_output.streams {
        // Check if this codec is text-based
        let is_text = matches!(
            stream.codec_name.as_str(),
            "subrip" | "ass" | "ssa" | "webvtt" | "mov_text" | "srt" | "text"
        );

        if is_text {
            // Extract language from tags
            let language = stream
                .tags
                .get("language")
                .filter(|lang| !lang.is_empty() && *lang != "und")
                .cloned();

            // Create display title
            let display_title = format_display_title(subtitle_index, &stream.codec_name, &language);

            streams.push(SubtitleStream {
                index: subtitle_index,
                codec_name: stream.codec_name,
                language,
                display_title,
            });

            subtitle_index += 1;
        }
    }

    streams
}

/// Format a display title for the subtitle stream
fn format_display_title(index: usize, codec_name: &str, language: &Option<String>) -> String {
    let codec_display = match codec_name {
        "subrip" | "srt" => "SRT",
        "ass" | "ssa" => "ASS",
        "webvtt" => "WebVTT",
        "mov_text" => "MOV Text",
        _ => codec_name,
    };

    match language {
        Some(lang) => format!("Subtitle {} - {} ({})", index + 1, lang.to_uppercase(), codec_display),
        None => format!("Subtitle {} ({})", index + 1, codec_display),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_empty_json() {
        let json = r#"{"streams": []}"#;
        let streams = parse_ffprobe_json(json);
        assert_eq!(streams.len(), 0);
    }

    #[test]
    fn test_parse_single_subtitle() {
        let json = r#"{"streams": [{"index": 2, "codec_name": "subrip", "tags": {"language": "eng"}}]}"#;
        let streams = parse_ffprobe_json(json);
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].index, 0); // Subtitle index, not stream index
        assert_eq!(streams[0].codec_name, "subrip");
        assert_eq!(streams[0].language, Some("eng".to_string()));
    }

    #[test]
    fn test_filter_non_text_subtitles() {
        let json = r#"{"streams": [{"index": 2, "codec_name": "subrip", "tags": {}}, {"index": 3, "codec_name": "dvd_subtitle", "tags": {}}]}"#;
        let streams = parse_ffprobe_json(json);
        assert_eq!(streams.len(), 1);
        assert_eq!(streams[0].codec_name, "subrip");
    }

    #[test]
    fn test_multiple_subtitle_streams() {
        let json = r#"{"streams": [{"index": 2, "codec_name": "subrip", "tags": {"language": "eng"}}, {"index": 3, "codec_name": "subrip", "tags": {"language": "spa"}}]}"#;
        let streams = parse_ffprobe_json(json);
        assert_eq!(streams.len(), 2);
        assert_eq!(streams[0].language, Some("eng".to_string()));
        assert_eq!(streams[1].language, Some("spa".to_string()));
    }
}
