//! ASVE - Video Editor with GPUI
//!
//! A simple video player application built with GPUI and mpv (with libplacebo rendering).

#[cfg(target_os = "macos")]
#[macro_use]
extern crate objc;

use clap::Parser;

mod checkbox;
mod controls_window;
mod custom_titlebar;
mod ffmpeg_export;
mod font_utils;
mod initial_window;
mod input;
mod platform;
mod select;
mod slider;
mod subtitle_clip_tab;
mod subtitle_detector;
mod subtitle_extractor;
mod subtitle_window;
mod theme;
mod time_input;
mod unified_window;
mod video_player;
mod video_player_window;
mod virtual_list;

use gpui::{
    actions, px, AnyWindowHandle, App, AppContext, Application, BorrowAppContext, Global, Menu,
    MenuItem, PathPromptOptions, SystemMenuType, WindowOptions,
};
use initial_window::InitialWindow;
use unified_window::UnifiedWindow;
use video_player::ClockTime;

use std::sync::{Arc, Mutex};

#[derive(Parser, Debug)]
#[command(name = "asve")]
#[command(about = "ASVE - Video Editor with GPUI", long_about = None)]
struct Cli {
    /// Path to video file to open
    video_path: Option<String>,

    /// Clip start time (supports: 90.5, 01:30.500, 00:01:30.500, or 90500)
    #[arg(long)]
    clip_start: Option<String>,

    /// Clip end time (supports: 120.75, 02:00.750, 00:02:00.750, or 120750)
    #[arg(long)]
    clip_end: Option<String>,
}

/// Parse a timestamp string into milliseconds
///
/// Supports multiple formats:
/// - Seconds (decimal): "90.5" → 90,500 ms
/// - MM:SS.mmm: "01:30.500" → 90,500 ms
/// - HH:MM:SS.mmm: "00:01:30.500" → 90,500 ms
/// - Milliseconds (integer): "90500" → 90,500 ms
fn parse_timestamp(input: &str) -> Result<f32, String> {
    let input = input.trim();

    // Count colons to determine format
    let colon_count = input.matches(':').count();

    match colon_count {
        0 => {
            // Either seconds (decimal) or milliseconds (integer)
            if let Ok(value) = input.parse::<f32>() {
                if value < 0.0 {
                    return Err(format!("Timestamp cannot be negative: {}", input));
                }
                // If it contains a decimal point, treat as seconds
                // Otherwise, treat as milliseconds if > 1000, seconds if <= 1000
                if input.contains('.') {
                    // Decimal seconds
                    Ok(value * 1000.0)
                } else if value > 1000.0 {
                    // Likely milliseconds
                    Ok(value)
                } else {
                    // Small integer, treat as seconds
                    Ok(value * 1000.0)
                }
            } else {
                Err(format!("Invalid number format: {}", input))
            }
        }
        1 => {
            // MM:SS.mmm format
            let parts: Vec<&str> = input.split(':').collect();
            if parts.len() != 2 {
                return Err(format!("Invalid MM:SS.mmm format: {}", input));
            }

            let minutes = parts[0]
                .parse::<u32>()
                .map_err(|_| format!("Invalid minutes: {}", parts[0]))?;

            // Parse seconds and milliseconds
            let (seconds, milliseconds) = if parts[1].contains('.') {
                let sec_parts: Vec<&str> = parts[1].split('.').collect();
                if sec_parts.len() != 2 {
                    return Err(format!("Invalid seconds format: {}", parts[1]));
                }
                let secs = sec_parts[0]
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid seconds: {}", sec_parts[0]))?;
                let ms_str = format!("{:0<3}", sec_parts[1]); // Pad to 3 digits
                let ms = ms_str[..3]
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid milliseconds: {}", sec_parts[1]))?;
                (secs, ms)
            } else {
                let secs = parts[1]
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid seconds: {}", parts[1]))?;
                (secs, 0)
            };

            if seconds >= 60 {
                return Err(format!("Seconds must be less than 60: {}", seconds));
            }

            let total_ms = (minutes * 60 * 1000) + (seconds * 1000) + milliseconds;
            Ok(total_ms as f32)
        }
        2 => {
            // HH:MM:SS.mmm format
            let parts: Vec<&str> = input.split(':').collect();
            if parts.len() != 3 {
                return Err(format!("Invalid HH:MM:SS.mmm format: {}", input));
            }

            let hours = parts[0]
                .parse::<u32>()
                .map_err(|_| format!("Invalid hours: {}", parts[0]))?;
            let minutes = parts[1]
                .parse::<u32>()
                .map_err(|_| format!("Invalid minutes: {}", parts[1]))?;

            if minutes >= 60 {
                return Err(format!("Minutes must be less than 60: {}", minutes));
            }

            // Parse seconds and milliseconds
            let (seconds, milliseconds) = if parts[2].contains('.') {
                let sec_parts: Vec<&str> = parts[2].split('.').collect();
                if sec_parts.len() != 2 {
                    return Err(format!("Invalid seconds format: {}", parts[2]));
                }
                let secs = sec_parts[0]
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid seconds: {}", sec_parts[0]))?;
                let ms_str = format!("{:0<3}", sec_parts[1]); // Pad to 3 digits
                let ms = ms_str[..3]
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid milliseconds: {}", sec_parts[1]))?;
                (secs, ms)
            } else {
                let secs = parts[2]
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid seconds: {}", parts[2]))?;
                (secs, 0)
            };

            if seconds >= 60 {
                return Err(format!("Seconds must be less than 60: {}", seconds));
            }

            let total_ms =
                (hours * 60 * 60 * 1000) + (minutes * 60 * 1000) + (seconds * 1000) + milliseconds;
            Ok(total_ms as f32)
        }
        _ => Err(format!(
            "Invalid timestamp format (too many colons): {}",
            input
        )),
    }
}

fn main() {
    let cli = Cli::parse();
    // Initialize mpv before creating the GPUI application
    if let Err(e) = video_player::init() {
        eprintln!("Failed to initialize mpv: {}", e);
        eprintln!("Make sure mpv is installed: brew install mpv");
        std::process::exit(1);
    }

    // Parse and validate clip times if provided
    let parsed_clip_start: Option<f32>;
    let parsed_clip_end: Option<f32>;

    // Validate that clip options are only used with a video file
    if (cli.clip_start.is_some() || cli.clip_end.is_some()) && cli.video_path.is_none() {
        eprintln!(
            "Error: --clip-start and --clip-end can only be used when providing a video file"
        );
        std::process::exit(1);
    }

    // Parse clip times
    if let Some(ref start_str) = cli.clip_start {
        match parse_timestamp(start_str) {
            Ok(ms) => parsed_clip_start = Some(ms),
            Err(e) => {
                eprintln!("Error parsing --clip-start: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        parsed_clip_start = None;
    }

    if let Some(ref end_str) = cli.clip_end {
        match parse_timestamp(end_str) {
            Ok(ms) => parsed_clip_end = Some(ms),
            Err(e) => {
                eprintln!("Error parsing --clip-end: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        parsed_clip_end = None;
    }

    // Validate that both clip start and end are provided together
    if parsed_clip_start.is_some() != parsed_clip_end.is_some() {
        eprintln!("Error: Both --clip-start and --clip-end must be provided together");
        std::process::exit(1);
    }

    // Validate that clip_start < clip_end
    if let (Some(start), Some(end)) = (parsed_clip_start, parsed_clip_end) {
        if start >= end {
            eprintln!(
                "Error: --clip-start must be less than --clip-end ({} >= {})",
                start, end
            );
            std::process::exit(1);
        }
    }

    Application::new().run(move |cx: &mut App| {
        cx.set_global(AppState::new());

        // Bring the menu bar to the foreground (so you can see the menu bar)
        cx.activate(true);
        // Register the `quit` function so it can be referenced by the `MenuItem::action` in the menu bar
        cx.on_action(quit);
        cx.on_action(open_file);

        // Bind keys for time input
        cx.bind_keys([gpui::KeyBinding::new(
            "backspace",
            time_input::Backspace,
            Some("TimeInput"),
        )]);

        // Bind keys for input component
        cx.bind_keys([
            gpui::KeyBinding::new("backspace", input::state::Backspace, Some("InputState")),
            gpui::KeyBinding::new("delete", input::state::Delete, Some("InputState")),
            gpui::KeyBinding::new("enter", input::state::Enter, Some("InputState")),
            gpui::KeyBinding::new("escape", input::state::Escape, Some("InputState")),
            gpui::KeyBinding::new("left", input::state::Left, Some("InputState")),
            gpui::KeyBinding::new("right", input::state::Right, Some("InputState")),
            gpui::KeyBinding::new("up", input::state::Up, Some("InputState")),
            gpui::KeyBinding::new("down", input::state::Down, Some("InputState")),
            gpui::KeyBinding::new("cmd-a", input::state::SelectAll, Some("InputState")),
            gpui::KeyBinding::new("cmd-c", input::state::Copy, Some("InputState")),
            gpui::KeyBinding::new("cmd-x", input::state::Cut, Some("InputState")),
            gpui::KeyBinding::new("cmd-v", input::state::Paste, Some("InputState")),
        ]);

        // Add menu items
        set_app_menus(cx);

        // Check if a video path was provided via command line
        if let Some(video_path) = cli.video_path {
            // Validate the file exists and has a supported extension
            let path = std::path::Path::new(&video_path);
            if !path.exists() {
                eprintln!("Error: File does not exist: {}", video_path);
                std::process::exit(1);
            }

            let extension = path.extension().and_then(|e| e.to_str());
            let supported_extensions = ffmpeg_export::get_video_extensions();

            if let Some(ext) = extension {
                let ext_lower = ext.to_lowercase();
                if supported_extensions.contains(&ext_lower.as_str()) {
                    // Open the video directly
                    println!("Opening video file: {}", video_path);
                    let path_clone = video_path.clone();
                    create_video_windows(
                        cx,
                        video_path,
                        path_clone,
                        parsed_clip_start,
                        parsed_clip_end,
                    );
                } else {
                    eprintln!(
                        "Error: Invalid file type. Supported formats: {}",
                        supported_extensions.join(", ")
                    );
                    std::process::exit(1);
                }
            } else {
                eprintln!("Error: File has no extension");
                std::process::exit(1);
            }
        } else {
            // No video path provided, create the unified window with disabled controls and ASCII portal
            let total_width = 1260.0;
            let total_height = 720.0;

            let unified_window_options = WindowOptions {
                window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                    None,
                    gpui::size(px(total_width), px(total_height)),
                    cx,
                ))),
                window_background: gpui::WindowBackgroundAppearance::Opaque,
                focus: true,
                is_movable: true,
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("asve".into()),
                    appears_transparent: true,
                    traffic_light_position: Some(gpui::point(px(8.0), px(12.0))),
                    ..Default::default()
                }),
                ..Default::default()
            };

            let window = cx
                .open_window(unified_window_options, |_window, cx| {
                    cx.new(|cx| UnifiedWindow::new(cx))
                })
                .unwrap();

            // Store the unified window handle
            cx.update_global::<AppState, _>(|state, _| {
                state.unified_window = Some(window.into());
            });

            println!("Unified window created (no video loaded)");
        }
    });
}

/// Extract the native window handle from GPUI and create a child window/view for video rendering
///
/// This function uses the stored AnyWindowHandle to access the unified window's window_handle()
/// method, which provides raw window handle access via the raw-window-handle crate.
/// - On macOS, this creates a child NSView for mpv rendering
/// - On Windows, this creates a child HWND for mpv rendering
///
/// Returns the child window/view handle as a usize if successful.
fn extract_and_set_display_handle(cx: &mut App) -> Option<usize> {
    let app_state = cx.global::<AppState>();

    if let Some(window_handle) = app_state.unified_window() {
        let video_player = app_state.video_player.clone();

        // Access the window through the handle to create the platform-specific child surface
        let result = window_handle
            .update(cx, |_view, window, _app| {
                platform::create_child_video_surface(window, video_player)
            })
            .ok()
            .and_then(|x| x);

        return result;
    } else {
        eprintln!("No unified window handle stored in AppState");
        None
    }
}

/// Subtitle styling settings
#[derive(Clone, Debug)]
pub struct SubtitleSettings {
    pub font_family: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
    pub color: String,
}

impl SubtitleSettings {
    fn default() -> Self {
        Self {
            font_family: "Arial".to_string(),
            font_size: 55.0,
            bold: false,
            italic: false,
            color: "#FFFFFF".to_string(),
        }
    }
}

pub struct AppState {
    pub file_path: Option<String>,
    pub initial_window: Option<AnyWindowHandle>,
    pub unified_window: Option<AnyWindowHandle>,
    pub video_nsview: Option<usize>, // Pointer to the child NSView for video rendering
    pub video_player: Arc<Mutex<video_player::VideoPlayer>>,
    pub synced_to_video: bool,
    pub selected_subtitle_track: Option<usize>, // Currently selected subtitle track index
    pub display_subtitles: bool,
    pub subtitle_settings: SubtitleSettings,
    pub source_video_width: u32, // Horizontal resolution of the source video for subtitle scaling
    pub has_video_loaded: bool,  // Whether a video has been loaded
    pub custom_subtitle_mode: bool, // Whether custom subtitle mode is enabled in clip tab
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            initial_window: None,
            unified_window: None,
            video_nsview: None,
            video_player: Arc::new(Mutex::new(video_player::VideoPlayer::new())),
            synced_to_video: true,            // Default to checked/synced
            selected_subtitle_track: Some(0), // No track selected initially
            display_subtitles: false,
            subtitle_settings: SubtitleSettings::default(),
            source_video_width: 1920, // Default to 1920 (will be updated when video loads)
            has_video_loaded: false,  // No video loaded initially
            custom_subtitle_mode: false, // Default to off
        }
    }

    /// Get the unified window handle
    pub fn unified_window(&self) -> Option<AnyWindowHandle> {
        self.unified_window
    }
}

impl Global for AppState {}

fn set_app_menus(cx: &mut App) {
    cx.set_menus(vec![Menu {
        name: "set_menus".into(),
        items: vec![
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Open...", OpenFile),
            MenuItem::separator(),
            MenuItem::action("Quit", Quit),
        ],
    }]);
}

// Associate actions using the `actions!` macro (or `Action` derive macro)
actions!(set_menus, [Quit, OpenFile]);

// Define the quit function that is registered with the App
fn quit(_: &Quit, cx: &mut App) {
    println!("Gracefully quitting the application . . .");
    cx.quit();
}

/// Create the unified video player window and load the video file
pub fn create_video_windows(
    cx: &mut App,
    path_string: String,
    path_clone: String,
    clip_start: Option<f32>,
    clip_end: Option<f32>,
) {
    // Get handles to existing windows before creating new ones
    println!("Preparing to create new video windows");

    // Get handles before clearing state
    let app_state = cx.global::<AppState>();
    let initial_window = app_state.initial_window;
    let old_unified_window = app_state.unified_window;

    // Extract just the file name from the path for the window title
    let file_name = std::path::Path::new(&path_string)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Video Player")
        .to_string();

    // Calculate unified window size
    // Total width: video (960) + subtitle (300) = 1260
    // Total height: video section (540) + controls (180) = 720
    let total_width = 1260.0;
    let total_height = 720.0;

    // Create the unified window FIRST before closing the initial window
    let unified_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(20.0), px(20.0)),
            size: gpui::size(px(total_width), px(total_height)),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: true,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(file_name.clone().into()),
            appears_transparent: true,
            traffic_light_position: Some(gpui::point(px(8.0), px(12.0))),
            ..Default::default()
        }),
        ..Default::default()
    };

    let unified_window = cx
        .open_window(unified_window_options, |_window, cx| {
            cx.new(|cx| UnifiedWindow::new(cx))
        })
        .unwrap();

    println!("Unified window created");

    // Now that the new window is created, close the old windows
    println!("Closing old windows");

    if let Some(window) = initial_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = old_unified_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }

    // Get video resolution before updating AppState
    let (video_width, _video_height) =
        crate::ffmpeg_export::get_video_resolution(&path_string).unwrap_or((1920, 1080));

    // Update AppState with new window, file path, and source video resolution
    cx.update_global::<AppState, _>(|state, _| {
        state.initial_window = None;
        state.unified_window = Some(unified_window.into());
        state.video_nsview = None;
        state.file_path = Some(path_string.clone());
        state.source_video_width = video_width;
        state.has_video_loaded = true; // Mark that a video has been loaded
    });

    // Update the titlebar with the filename
    unified_window
        .update(cx, |unified_window, _, cx| {
            unified_window.titlebar.update(cx, |titlebar, cx| {
                titlebar.set_title(file_name.clone());
                cx.notify();
            });
        })
        .ok();

    // Set initial clip times if provided via CLI
    if let (Some(start_ms), Some(end_ms)) = (clip_start, clip_end) {
        unified_window
            .update(cx, |unified_window, _, cx| {
                let controls_entity = unified_window.controls.clone();
                controls_entity.update(cx, |controls, cx| {
                    // Convert f32 milliseconds to u64 for set_clip_times
                    controls.set_clip_times(start_ms as u64, end_ms as u64, cx);
                    println!("Set initial clip times: {} ms to {} ms", start_ms, end_ms);
                });
            })
            .ok();
    }

    // Extract and set the display handle for the video window
    if let Some(child_view_ptr) = extract_and_set_display_handle(cx) {
        // Store the child NSView pointer in AppState
        cx.update_global::<AppState, _>(|state, _| {
            state.video_nsview = Some(child_view_ptr);
        });
    }

    // Load the video file
    let video_player = cx.global::<AppState>().video_player.clone();
    if let Ok(mut player) = video_player.lock() {
        println!("Loading video file: {}", path_clone);

        // Load the file into the pipeline
        match player.load_file(&path_clone) {
            Ok(()) => {
                println!("Video file loaded successfully");

                // Start the bus watch to handle messages via GLib main loop
                match player.start_message_watch() {
                    Ok(()) => {
                        println!("Bus watch started successfully");
                    }
                    Err(e) => {
                        eprintln!("Failed to start bus watch: {}", e);
                    }
                }

                // Auto-play and immediately pause to get duration information
                if let Err(e) = player.play() {
                    eprintln!("Failed to auto-play: {}", e);
                } else {
                    println!("Auto-played video to get duration");
                    // Immediately pause
                    if let Err(e) = player.pause() {
                        eprintln!("Failed to pause: {}", e);
                    } else {
                        println!("Video paused and ready");
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to load video file: {}", e);
            }
        }
    };

    // Seek to clip start if provided via CLI (with delay to allow mpv to be ready)
    // there must be some better way to do this, but this does consistently work
    if let Some(start_ms) = clip_start {
        let video_player = cx.global::<AppState>().video_player.clone();
        cx.spawn(async move |_cx| {
            // Wait a bit for mpv to be fully ready
            use std::time::Duration;
            std::thread::sleep(Duration::from_millis(100));

            if let Ok(player) = video_player.lock() {
                // Convert milliseconds to nanoseconds
                let nanos = (start_ms * 1_000_000.0) as u64;
                let clock_time = ClockTime::from_nseconds(nanos);
                if let Err(e) = player.seek(clock_time) {
                    eprintln!("Failed to seek to clip start: {}", e);
                } else {
                    println!("Seeked to clip start: {} ms", start_ms);
                }
            }
        })
        .detach();
    }

    // Load subtitle streams in the unified window on a background thread
    let unified_window_handle = cx.global::<AppState>().unified_window;
    if let Some(window_handle) = unified_window_handle {
        let path_for_subtitles = path_clone.clone();

        cx.spawn(async move |cx| {
            // Run blocking subtitle loading on background executor
            let subtitle_data = cx
                .background_executor()
                .spawn(async move {
                    println!("Loading subtitles on background thread...");
                    crate::subtitle_window::SubtitleWindow::load_subtitle_data_blocking(
                        &path_for_subtitles,
                    )
                })
                .await;

            // Update UI on main thread with loaded data
            if let Some(data) = subtitle_data {
                cx.update(|cx| {
                    window_handle
                        .update(cx, |any_view, _, app_cx| {
                            if let Ok(unified_window) = any_view.downcast::<UnifiedWindow>() {
                                // First get the subtitle entity, then update it
                                let subtitle_entity = unified_window.read(app_cx).subtitles.clone();
                                subtitle_entity.update(app_cx, |subtitle_window, cx| {
                                    println!("Updating subtitle window with loaded data");
                                    subtitle_window.update_with_loaded_data(data, cx);
                                });
                            }
                        })
                        .ok();
                })
                .ok();
            } else {
                println!("No subtitle data loaded");
            }
        })
        .detach();
    }
}

// Define the open file function that prompts for a file path
fn open_file(_: &OpenFile, cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Select a video file".into()),
    });

    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = paths.await {
            if let Some(path) = paths.first() {
                // Check if the file has a valid extension
                let extension = path.extension().and_then(|e| e.to_str());
                let supported_extensions = ffmpeg_export::get_video_extensions();

                if let Some(ext) = extension {
                    let ext_lower = ext.to_lowercase();
                    if supported_extensions.contains(&ext_lower.as_str()) {
                        let path_string = path.to_string_lossy().to_string();
                        let path_clone = path_string.clone();

                        cx.update(|cx| {
                            create_video_windows(cx, path_string, path_clone, None, None);
                        })
                        .ok();
                    } else {
                        // Invalid file type
                        eprintln!(
                            "Invalid file type. Supported formats: {}",
                            supported_extensions.join(", ")
                        );
                    }
                }
            }
        }
    })
    .detach();
}
