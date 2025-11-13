//! ASVE - Video Editor with GPUI
//!
//! A simple video player application built with GPUI and GStreamer.

mod checkbox;
mod controls_window;
mod ffmpeg_export;
mod initial_window;
mod search_input;
mod select;
mod slider;
mod subtitle_detector;
mod subtitle_extractor;
mod subtitle_window;
mod video_player;
mod video_player_window;

use controls_window::ControlsWindow;
use gpui::{
    actions, AnyWindowHandle, App, AppContext, Application, BorrowAppContext, Global, Menu,
    MenuItem, PathPromptOptions, SystemMenuType, WindowOptions, px,
};
use initial_window::InitialWindow;
use raw_window_handle::RawWindowHandle;
use subtitle_window::SubtitleWindow;
use video_player_window::VideoPlayerWindow;

use std::sync::{Arc, Mutex};


fn main() {
    // Initialize GStreamer before creating the GPUI application
    if let Err(e) = video_player::init() {
        eprintln!("Failed to initialize GStreamer: {}", e);
        eprintln!(
            "Make sure GStreamer is installed: brew install gstreamer gst-plugins-base gst-plugins-good"
        );
        std::process::exit(1);
    }

    Application::new().run(|cx: &mut App| {
        cx.set_global(AppState::new());

        // Bring the menu bar to the foreground (so you can see the menu bar)
        cx.activate(true);
        // Register the `quit` function so it can be referenced by the `MenuItem::action` in the menu bar
        cx.on_action(quit);
        cx.on_action(open_file);

        // Bind keys for search input
        cx.bind_keys([
            gpui::KeyBinding::new("enter", search_input::Enter, Some("SearchInput")),
            gpui::KeyBinding::new("escape", search_input::Escape, Some("SearchInput")),
            gpui::KeyBinding::new("backspace", search_input::Backspace, Some("SearchInput")),
        ]);

        // Add menu items
        set_app_menus(cx);

        // Create a small initial window with just the "Open File" button
        let initial_window_options = WindowOptions {
            window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
                None,
                gpui::size(px(300.0), px(200.0)),
                cx,
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("asve".into()),
                appears_transparent: false,
                ..Default::default()
            }),
            ..Default::default()
        };

        let window = cx
            .open_window(initial_window_options, |_window, cx| {
                cx.new(|_| InitialWindow {})
            })
            .unwrap();

        // Store the initial window handle
        cx.update_global::<AppState, _>(|state, _| {
            state.initial_window = Some(window.into());
        });

        println!("Initial window created");
    });
}

/// Extract the native NSView handle from GPUI and set it on the video player
///
/// This function uses the stored AnyWindowHandle to access the window's window_handle()
/// method, which provides raw window handle access via the raw-window-handle crate.
/// On macOS, this extracts the NSView pointer needed for GStreamer video rendering.
fn extract_and_set_display_handle(cx: &mut App) {
    let app_state = cx.global::<AppState>();

    if let Some(window_handle) = app_state.video_window() {
        let video_player = app_state.video_player.clone();

        // Access the window through the handle to get the window handle
        window_handle
            .update(cx, |_view, window, _app| {
                // Get the raw window handle from the window using the HasWindowHandle trait
                use raw_window_handle::HasWindowHandle;
                match window.window_handle() {
                    Ok(window_handle_obj) => {
                        // Extract the platform-specific handle
                        let raw_handle = window_handle_obj.as_raw();

                        match raw_handle {
                            RawWindowHandle::AppKit(appkit_handle) => {
                                // Extract the NSView pointer from the AppKit handle
                                // The ns_view field is a NonNull<c_void> which is safe to access
                                let ns_view_ptr = appkit_handle.ns_view.as_ptr() as usize;

                                println!("NSView pointer extracted: 0x{:x}", ns_view_ptr);

                                // Get the window bounds to calculate render rectangle
                                let bounds = window.bounds();
                                // Pixels is a wrapper around f32, we need to extract the value
                                // Using format! to convert to string then parse is a workaround
                                let width_str = format!("{}", bounds.size.width);
                                let height_str = format!("{}", bounds.size.height);
                                let window_width: i32 =
                                    width_str.trim_end_matches("px").parse().unwrap_or(800);
                                let window_height: i32 =
                                    height_str.trim_end_matches("px").parse().unwrap_or(600);

                                println!("Window size: {}x{}", window_width, window_height);

                                // Pass the NSView pointer and render rectangle to the video player
                                if let Ok(mut player) = video_player.lock() {
                                    player.set_window_handle(ns_view_ptr);
                                    println!(
                                        "NSView pointer and render rectangle set on video player"
                                    );
                                } else {
                                    eprintln!("Failed to lock video player mutex");
                                }
                            }
                            _ => {
                                eprintln!(
                                    "Unsupported platform window handle type: {:?}",
                                    raw_handle
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to get window handle: {:?}", e);
                    }
                }
            })
            .ok();
    } else {
        eprintln!("No video window handle stored in AppState");
    }
}

pub struct AppState {
    pub file_path: Option<String>,
    pub initial_window: Option<AnyWindowHandle>,
    pub video_window: Option<AnyWindowHandle>,
    pub controls_window: Option<AnyWindowHandle>,
    pub subtitle_window: Option<AnyWindowHandle>,
    pub video_player: Arc<Mutex<video_player::VideoPlayer>>,
    pub synced_to_video: bool,
    pub selected_subtitle_track: Option<usize>, // Currently selected subtitle track index
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            initial_window: None,
            video_window: None,
            controls_window: None,
            subtitle_window: None,
            video_player: Arc::new(Mutex::new(video_player::VideoPlayer::new())),
            synced_to_video: true, // Default to checked/synced
            selected_subtitle_track: None, // No track selected initially
        }
    }

    /// Get the video window handle
    pub fn video_window(&self) -> Option<AnyWindowHandle> {
        self.video_window
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

/// Create the video player and controls windows and load the video file
pub fn create_video_windows(cx: &mut App, path_string: String, path_clone: String) {
    // Close existing windows by calling remove_window()
    println!("Closing existing windows");

    // Get handles before clearing state
    let app_state = cx.global::<AppState>();
    let initial_window = app_state.initial_window;
    let video_window = app_state.video_window;
    let controls_window = app_state.controls_window;
    let subtitle_window = app_state.subtitle_window;

    // Close the windows by calling remove_window() on each
    if let Some(window) = initial_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = video_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = controls_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = subtitle_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }

    // Clear the handles from state
    cx.update_global::<AppState, _>(|state, _| {
        state.initial_window = None;
        state.video_window = None;
        state.controls_window = None;
        state.subtitle_window = None;
    });

    // Extract just the file name from the path for the window title
    let file_name = std::path::Path::new(&path_string)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Video Player")
        .to_string();

    // Calculate video window size (half of typical 1920px screen, maintain 16:9 aspect ratio)
    let video_width = 960.0;
    let video_height = video_width * 9.0 / 16.0; // 540px to maintain 16:9 aspect ratio

    // Create the video player window (closable)
    let video_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(20.0), px(20.0)), // Start at top of screen with small margin
            size: gpui::size(px(video_width), px(video_height)),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: true,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some(file_name.into()),
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let video_window = cx
        .open_window(video_window_options, |_window, cx| {
            cx.new(|_| VideoPlayerWindow {})
        })
        .unwrap();

    println!("Video window created");

    // Get the video window's bounds to position controls below it
    let video_bounds = video_window
        .update(cx, |_, window, _| window.bounds())
        .unwrap();

    // Calculate position for controls window (directly below video window)
    let controls_x = video_bounds.origin.x;
    let controls_y = video_bounds.origin.y + video_bounds.size.height;
    let controls_width = video_bounds.size.width; // Match video window width

    // Create the controls window (not closable, no title)
    let controls_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(controls_x, controls_y),
            size: gpui::size(controls_width, px(180.0)),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: false,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: None,
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let controls_window = cx
        .open_window(controls_window_options, |_window, cx| {
            cx.new(|cx| ControlsWindow::new(cx))
        })
        .unwrap();

    println!("Controls window created");

    // Calculate position for subtitle window (to the right of video window)
    let subtitle_x = video_bounds.origin.x + video_bounds.size.width;
    let subtitle_y = video_bounds.origin.y;
    let subtitle_width = px(300.0); // Proportionally scaled subtitle window width
    let subtitle_height = video_bounds.size.height; // Match video window height

    // Create the subtitle window
    let subtitle_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(subtitle_x, subtitle_y),
            size: gpui::size(subtitle_width, subtitle_height),
        })),
        window_background: gpui::WindowBackgroundAppearance::Opaque,
        focus: false,
        is_movable: true,
        titlebar: Some(gpui::TitlebarOptions {
            title: Some("Subtitles".into()),
            appears_transparent: false,
            ..Default::default()
        }),
        ..Default::default()
    };

    let subtitle_window = cx
        .open_window(subtitle_window_options, |_window, cx| {
            cx.new(|cx| SubtitleWindow::new(cx))
        })
        .unwrap();

    println!("Subtitle window created");

    // Update AppState with new windows and file path
    cx.update_global::<AppState, _>(|state, _| {
        state.video_window = Some(video_window.into());
        state.controls_window = Some(controls_window.into());
        state.subtitle_window = Some(subtitle_window.into());
        state.file_path = Some(path_string.clone());
    });

    // Extract and set the display handle for the video window
    extract_and_set_display_handle(cx);

    // Load the video file
    let app_state = cx.global::<AppState>();
    let video_player = app_state.video_player.clone();
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
    }

    // Load subtitle streams in the subtitle window
    if let Some(subtitle_window_handle) = cx.global::<AppState>().subtitle_window {
        subtitle_window_handle
            .update(cx, |any_view, _, app_cx| {
                if let Ok(subtitle_window) = any_view.downcast::<SubtitleWindow>() {
                    subtitle_window.update(app_cx, |subtitle_window, cx| {
                        subtitle_window.load_subtitle_streams(&path_clone, cx);
                    });
                }
            })
            .ok();
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
                            create_video_windows(cx, path_string, path_clone);
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
