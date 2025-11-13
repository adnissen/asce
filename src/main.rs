//! ASVE - Video Editor with GPUI
//!
//! A simple video player application built with GPUI and mpv (with libplacebo rendering).

#[macro_use]
extern crate objc;

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
mod time_input;
mod unified_window;
mod video_player;
mod video_player_window;

use gpui::{
    actions, AnyWindowHandle, App, AppContext, Application, BorrowAppContext, Global, Menu,
    MenuItem, PathPromptOptions, SystemMenuType, WindowOptions, px,
};
use initial_window::InitialWindow;
use raw_window_handle::RawWindowHandle;
use unified_window::UnifiedWindow;

use std::sync::{Arc, Mutex};


fn main() {
    // Initialize mpv before creating the GPUI application
    if let Err(e) = video_player::init() {
        eprintln!("Failed to initialize mpv: {}", e);
        eprintln!("Make sure mpv is installed: brew install mpv");
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

        // Bind keys for time input
        cx.bind_keys([
            gpui::KeyBinding::new("backspace", time_input::Backspace, Some("TimeInput")),
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

/// Extract the native NSView handle from GPUI and create a child NSView for video rendering
///
/// This function uses the stored AnyWindowHandle to access the unified window's window_handle()
/// method, which provides raw window handle access via the raw-window-handle crate.
/// On macOS, this extracts the parent NSView pointer and creates a child NSView for mpv rendering.
/// Returns the child NSView pointer if successful.
fn extract_and_set_display_handle(cx: &mut App) -> Option<usize> {
    let app_state = cx.global::<AppState>();

    if let Some(window_handle) = app_state.unified_window() {
        let video_player = app_state.video_player.clone();

        // Access the window through the handle to get the window handle
        let result = window_handle
            .update(cx, |_view, window, _app| {
                // Get the raw window handle from the window using the HasWindowHandle trait
                use raw_window_handle::HasWindowHandle;
                match window.window_handle() {
                    Ok(window_handle_obj) => {
                        // Extract the platform-specific handle
                        let raw_handle = window_handle_obj.as_raw();

                        match raw_handle {
                            RawWindowHandle::AppKit(appkit_handle) => Some({
                                // Extract the parent NSView pointer from the AppKit handle
                                let parent_ns_view_ptr = appkit_handle.ns_view.as_ptr();

                                println!("Parent NSView pointer extracted: 0x{:x}", parent_ns_view_ptr as usize);

                                // Get the unified window bounds to calculate video area size
                                let window_bounds = window.bounds();

                                // Calculate video area dimensions (76% width, 75% height)
                                let video_width_px = window_bounds.size.width * 0.76;
                                let video_height_px = window_bounds.size.height * 0.75;

                                // Extract numeric value from Pixels using format/parse (same as original code)
                                let width_str = format!("{}", video_width_px);
                                let height_str = format!("{}", video_height_px);
                                let window_height_str = format!("{}", window_bounds.size.height);
                                let video_width: f64 = width_str.trim_end_matches("px").parse().unwrap_or(960.0);
                                let video_height: f64 = height_str.trim_end_matches("px").parse().unwrap_or(540.0);
                                let window_height: f64 = window_height_str.trim_end_matches("px").parse().unwrap_or(720.0);

                                // Calculate Y position for NSView (bottom-left origin)
                                // Video should be in top 75% of window, which in bottom-left coordinates
                                // means it starts at 25% from the bottom
                                let video_y = window_height * 0.25;

                                println!("Video area size: {}x{} at y={}", video_width as i32, video_height as i32, video_y as i32);

                                // Create a child NSView for video rendering using Objective-C
                                unsafe {
                                    use objc::runtime::Object;

                                    // Get the NSView class
                                    let ns_view_class = objc::runtime::Class::get("NSView").unwrap();

                                    // Create a CGRect for the child view frame
                                    // NSView uses bottom-left origin, so we need to position it at 25% from bottom
                                    let frame = cocoa::foundation::NSRect::new(
                                        cocoa::foundation::NSPoint::new(0.0, video_y),
                                        cocoa::foundation::NSSize::new(video_width as f64, video_height as f64),
                                    );

                                    // Allocate and initialize the child NSView with the frame
                                    let child_view: *mut Object = msg_send![ns_view_class, alloc];
                                    let child_view: *mut Object = msg_send![child_view, initWithFrame:frame];

                                    if !child_view.is_null() {
                                        // Add the child view as a subview of the parent
                                        let parent_view = parent_ns_view_ptr as *mut Object;
                                        let _: () = msg_send![parent_view, addSubview:child_view];

                                        // Set wantsLayer to YES for better rendering performance
                                        let _: () = msg_send![child_view, setWantsLayer:true];

                                        let child_view_ptr = child_view as usize;
                                        println!("Child NSView created at: 0x{:x}", child_view_ptr);

                                        // Pass the child NSView pointer to the video player
                                        if let Ok(mut player) = video_player.lock() {
                                            player.set_window_handle(child_view_ptr);
                                            println!("Child NSView pointer set on video player");
                                        } else {
                                            eprintln!("Failed to lock video player mutex");
                                        }

                                        // Return the child view pointer so we can store it in AppState
                                        child_view_ptr
                                    } else {
                                        eprintln!("Failed to create child NSView");
                                        0  // Return 0 if failed
                                    }
                                }
                            }),
                            _ => {
                                eprintln!(
                                    "Unsupported platform window handle type: {:?}",
                                    raw_handle
                                );
                                None
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to get window handle: {:?}", e);
                        None
                    }
                }
            })
            .ok()
            .and_then(|x| x);

        return result;
    } else {
        eprintln!("No unified window handle stored in AppState");
        None
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
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            initial_window: None,
            unified_window: None,
            video_nsview: None,
            video_player: Arc::new(Mutex::new(video_player::VideoPlayer::new())),
            synced_to_video: true, // Default to checked/synced
            selected_subtitle_track: None, // No track selected initially
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
pub fn create_video_windows(cx: &mut App, path_string: String, path_clone: String) {
    // Close existing windows by calling remove_window()
    println!("Closing existing windows");

    // Get handles before clearing state
    let app_state = cx.global::<AppState>();
    let initial_window = app_state.initial_window;
    let unified_window = app_state.unified_window;

    // Close the windows by calling remove_window() on each
    if let Some(window) = initial_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }
    if let Some(window) = unified_window {
        window
            .update(cx, |_, window, _| {
                window.remove_window();
            })
            .ok();
    }

    // Clear the handles from state
    cx.update_global::<AppState, _>(|state, _| {
        state.initial_window = None;
        state.unified_window = None;
        state.video_nsview = None;
    });

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

    // Create the unified window
    let unified_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(20.0), px(20.0)),
            size: gpui::size(px(total_width), px(total_height)),
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

    let unified_window = cx
        .open_window(unified_window_options, |_window, cx| {
            cx.new(|cx| UnifiedWindow::new(cx))
        })
        .unwrap();

    println!("Unified window created");

    // Update AppState with new window and file path
    cx.update_global::<AppState, _>(|state, _| {
        state.unified_window = Some(unified_window.into());
        state.file_path = Some(path_string.clone());
    });

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

    // Load subtitle streams in the unified window on a background thread
    let unified_window_handle = cx.global::<AppState>().unified_window;
    if let Some(window_handle) = unified_window_handle {
        let path_for_subtitles = path_clone.clone();

        cx.spawn(async move |cx| {
            // Run blocking subtitle loading on background executor
            let subtitle_data = cx.background_executor().spawn(async move {
                println!("Loading subtitles on background thread...");
                crate::subtitle_window::SubtitleWindow::load_subtitle_data_blocking(&path_for_subtitles)
            }).await;

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
                }).ok();
            } else {
                println!("No subtitle data loaded");
            }
        }).detach();
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
