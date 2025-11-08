//! ASVE - Video Editor with GPUI
//!
//! A simple video player application built with GPUI and GStreamer.

mod video_player;

use gpui::{
    AnyWindowHandle, App, Application, Context, Global, Menu, MenuItem, PathPromptOptions,
    SystemMenuType, Window, WindowOptions, actions, div, prelude::*, rgb,
};
use raw_window_handle::RawWindowHandle;

use std::sync::{Arc, Mutex};

struct ASVE;

impl Render for ASVE {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_state = cx.global::<AppState>();
        let content = if let Some(ref path) = app_state.file_path {
            path.clone()
        } else {
            "No file path set. Please set a file path to continue.".to_string()
        };

        div()
            .flex()
            .bg(rgb(0x2e7d32))
            .size_full()
            .justify_center()
            .items_center()
            .text_xl()
            .text_color(rgb(0xffffff))
            .child(content)
    }
}

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
        // Add menu items
        set_app_menus(cx);
        let window = cx
            .open_window(WindowOptions::default(), |window, cx| {
                // Store the GPUI window handle in AppState for later use
                let gpui_window_handle = window.window_handle();
                cx.update_global::<AppState, _>(|state, _| {
                    state.window_handle = Some(gpui_window_handle);
                });
                return cx.new(|_| ASVE {});
            })
            .unwrap();

        // Log information about the window handle (for demonstration)
        println!("Window created with handle: {:?}", window);

        // Extract and set the NSView handle for the video player
        // Note: This requires accessing GPUI internals
        extract_and_set_display_handle(cx);
    });
}

/// Extract the native NSView handle from GPUI and set it on the video player
///
/// This function uses the stored AnyWindowHandle to access the window's window_handle()
/// method, which provides raw window handle access via the raw-window-handle crate.
/// On macOS, this extracts the NSView pointer needed for GStreamer video rendering.
fn extract_and_set_display_handle(cx: &mut App) {
    let app_state = cx.global::<AppState>();

    if let Some(window_handle) = app_state.window_handle() {
        let video_player = app_state.video_player.clone();

        // Access the window through the handle to get the window handle
        window_handle
            .update(cx, |_view, window, _app| {
                // Get the raw window handle from the window using the HasWindowHandle trait
                use raw_window_handle::HasWindowHandle;
                match window.window_handle() {
                    Ok(window_handle) => {
                        // Extract the platform-specific handle
                        let raw_handle = window_handle.as_raw();

                        match raw_handle {
                            RawWindowHandle::AppKit(appkit_handle) => {
                                // Extract the NSView pointer from the AppKit handle
                                // The ns_view field is a NonNull<c_void> which is safe to access
                                let ns_view_ptr = appkit_handle.ns_view.as_ptr() as usize;

                                println!("NSView pointer extracted: 0x{:x}", ns_view_ptr);

                                // Pass the NSView pointer to the video player
                                if let Ok(mut player) = video_player.lock() {
                                    player.set_window_handle(ns_view_ptr);
                                    println!("NSView pointer set on video player");
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
        eprintln!("No window handle stored in AppState");
    }
}

struct AppState {
    file_path: Option<String>,
    window_handle: Option<AnyWindowHandle>,
    video_player: Arc<Mutex<video_player::VideoPlayer>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            window_handle: None,
            video_player: Arc::new(Mutex::new(video_player::VideoPlayer::new())),
        }
    }

    /// Get the stored window handle
    pub fn window_handle(&self) -> Option<AnyWindowHandle> {
        self.window_handle
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

// Define the open file function that prompts for a file path
fn open_file(_: &OpenFile, cx: &mut App) {
    let paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Select an MP4 or MOV file".into()),
    });

    cx.spawn(async move |cx| {
        if let Ok(Ok(Some(paths))) = paths.await {
            if let Some(path) = paths.first() {
                // Check if the file has a valid extension
                let extension = path.extension().and_then(|e| e.to_str());
                match extension {
                    Some("mp4") | Some("mov") | Some("MP4") | Some("MOV") => {
                        let path_string = path.to_string_lossy().to_string();
                        let path_clone = path_string.clone();

                        cx.update(|cx| {
                            let app_state = cx.global_mut::<AppState>();
                            app_state.file_path = Some(path_string);

                            // Load and play the video
                            let video_player = app_state.video_player.clone();
                            if let Ok(mut player) = video_player.lock() {
                                println!("Loading video file: {}", path_clone);

                                // Load the file into the pipeline
                                match player.load_file(&path_clone) {
                                    Ok(()) => {
                                        println!("Video file loaded successfully");

                                        // Start playback
                                        match player.play() {
                                            Ok(()) => {
                                                println!("Video playback started");
                                            }
                                            Err(e) => {
                                                eprintln!("Failed to start playback: {}", e);
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("Failed to load video file: {}", e);
                                    }
                                }
                            }
                        })
                        .ok();
                    }
                    _ => {
                        // Invalid file type - could show an error dialog here
                        eprintln!("Invalid file type. Please select an .mp4 or .mov file.");
                    }
                }
            }
        }
    })
    .detach();
}
