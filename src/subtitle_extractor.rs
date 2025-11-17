//! Subtitle extraction and SRT parsing
//!
//! This module provides functionality to extract subtitle streams from video files
//! and parse them into a structured format for display.

use std::process::Command;

/// A single subtitle entry with timing and text
#[derive(Debug, Clone)]
pub struct SubtitleEntry {
    /// Start time in milliseconds
    pub start_ms: u64,
    /// End time in milliseconds
    pub end_ms: u64,
    /// Subtitle text content (may contain multiple lines)
    pub text: String,
}

impl SubtitleEntry {
    /// Format start time as SRT timecode (HH:MM:SS,mmm)
    pub fn format_start_time(&self) -> String {
        format_timecode(self.start_ms)
    }

    /// Format end time as SRT timecode (HH:MM:SS,mmm)
    pub fn format_end_time(&self) -> String {
        format_timecode(self.end_ms)
    }
}

/// Extract a subtitle stream from a video file and convert to SRT format
///
/// Uses ffmpeg to extract the specified subtitle stream and convert it to SRT.
/// Returns the SRT content as a string.
///
/// # Arguments
///
/// * `file_path` - Path to the video file
/// * `stream_index` - Index of the subtitle stream to extract (e.g., 0, 1, 2...)
///
/// # Returns
///
/// The SRT content as a string, or an error message if extraction fails.
pub fn extract_subtitle_stream(file_path: &str, stream_index: usize) -> Result<String, String> {
    // Use ffmpeg to extract the subtitle stream and convert to SRT
    let output = Command::new("ffmpeg")
        .args([
            "-i",
            file_path,
            "-map",
            &format!("0:s:{}", stream_index), // Map the specific subtitle stream
            "-f",
            "srt", // Output format: SRT
            "-",   // Output to stdout
        ])
        .output()
        .map_err(|e| format!("Failed to execute ffmpeg: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffmpeg failed: {}", stderr));
    }

    let srt_content = String::from_utf8_lossy(&output.stdout).to_string();

    if srt_content.trim().is_empty() {
        return Err("No subtitle content extracted".to_string());
    }

    Ok(srt_content)
}

/// Parse SRT content into a vector of SubtitleEntry structs
///
/// # Arguments
///
/// * `srt_content` - Raw SRT content as a string
///
/// # Returns
///
/// A vector of `SubtitleEntry` structs representing each subtitle.
pub fn parse_srt(srt_content: &str) -> Vec<SubtitleEntry> {
    let mut entries = Vec::new();
    let mut lines = srt_content.lines().peekable();

    while let Some(line) = lines.next() {
        let line = line.trim();

        // Skip empty lines
        if line.is_empty() {
            continue;
        }

        // Try to parse as entry index
        if line.parse::<usize>().is_ok() {
            // Next line should be timecode
            if let Some(timecode_line) = lines.next() {
                if let Some((start_ms, end_ms)) = parse_timecode_line(timecode_line) {
                    // Collect text lines until we hit an empty line or EOF
                    let mut text_lines = Vec::new();
                    while let Some(&next_line) = lines.peek() {
                        let next_line = next_line.trim();
                        if next_line.is_empty() {
                            lines.next(); // Consume the empty line
                            break;
                        }
                        text_lines.push(lines.next().unwrap().to_string());
                    }

                    let text = text_lines.join("\n");

                    entries.push(SubtitleEntry {
                        start_ms,
                        end_ms,
                        text,
                    });
                }
            }
        }
    }

    entries
}

/// Parse a SRT timecode line (e.g., "00:00:10,500 --> 00:00:13,000")
///
/// Returns (start_ms, end_ms) if parsing succeeds, None otherwise.
fn parse_timecode_line(line: &str) -> Option<(u64, u64)> {
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return None;
    }

    let start_ms = parse_timecode(parts[0].trim())?;
    let end_ms = parse_timecode(parts[1].trim())?;

    Some((start_ms, end_ms))
}

/// Parse a single SRT timecode (e.g., "00:00:10,500") into milliseconds
fn parse_timecode(timecode: &str) -> Option<u64> {
    // Format: HH:MM:SS,mmm
    let parts: Vec<&str> = timecode.split(':').collect();
    if parts.len() != 3 {
        return None;
    }

    let hours: u64 = parts[0].parse().ok()?;
    let minutes: u64 = parts[1].parse().ok()?;

    // Seconds and milliseconds are separated by comma
    let sec_parts: Vec<&str> = parts[2].split(',').collect();
    if sec_parts.len() != 2 {
        return None;
    }

    let seconds: u64 = sec_parts[0].parse().ok()?;
    let milliseconds: u64 = sec_parts[1].parse().ok()?;

    Some(hours * 3600000 + minutes * 60000 + seconds * 1000 + milliseconds)
}

/// Format milliseconds as SRT timecode (HH:MM:SS,mmm)
fn format_timecode(ms: u64) -> String {
    let hours = ms / 3600000;
    let minutes = (ms % 3600000) / 60000;
    let seconds = (ms % 60000) / 1000;
    let milliseconds = ms % 1000;

    format!(
        "{:02}:{:02}:{:02},{:03}",
        hours, minutes, seconds, milliseconds
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timecode() {
        assert_eq!(parse_timecode("00:00:10,500"), Some(10500));
        assert_eq!(parse_timecode("00:01:30,250"), Some(90250));
        assert_eq!(parse_timecode("01:23:45,678"), Some(5025678));
    }

    #[test]
    fn test_format_timecode() {
        assert_eq!(format_timecode(10500), "00:00:10,500");
        assert_eq!(format_timecode(90250), "00:01:30,250");
        assert_eq!(format_timecode(5025678), "01:23:45,678");
    }

    #[test]
    fn test_parse_timecode_line() {
        let line = "00:00:10,500 --> 00:00:13,000";
        assert_eq!(parse_timecode_line(line), Some((10500, 13000)));
    }

    #[test]
    fn test_parse_srt() {
        let srt = r#"1
00:00:10,500 --> 00:00:13,000
First subtitle line

2
00:00:15,000 --> 00:00:18,500
Second subtitle line
with multiple lines

"#;

        let entries = parse_srt(srt);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].start_ms, 10500);
        assert_eq!(entries[0].end_ms, 13000);
        assert_eq!(entries[0].text, "First subtitle line");

        assert_eq!(entries[1].start_ms, 15000);
        assert_eq!(entries[1].end_ms, 18500);
        assert_eq!(entries[1].text, "Second subtitle line\nwith multiple lines");
    }
}
