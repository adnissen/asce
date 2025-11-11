//! ASVE - Video Editor with GPUI
//!
//! A simple video player application built with GPUI and GStreamer.

mod slider;
mod video_player;

use gpui::{
    AnyWindowHandle, App, Application, Context, Entity, Global, Menu, MenuItem, PathPromptOptions,
    SystemMenuType, Window, WindowOptions, actions, div, prelude::*, px, rgb,
};
use raw_window_handle::RawWindowHandle;
use slider::{Slider, SliderEvent, SliderState, SliderValue};

use std::sync::{Arc, Mutex};

/// Initial window that shows just an "Open File" button
struct InitialWindow;

impl Render for InitialWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .bg(rgb(0x2e7d32))
            .size_full()
            .justify_center()
            .items_center()
            .child(
                div()
                    .id("open-file-button")
                    .px_8()
                    .py_4()
                    .bg(rgb(0x388e3c))
                    .rounded_lg()
                    .cursor_pointer()
                    .text_xl()
                    .text_color(rgb(0xffffff))
                    .hover(|style| style.bg(rgb(0x4caf50)))
                    .on_click(|_, _window, cx| {
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
                                                create_video_windows(cx, path_string, path_clone);
                                            })
                                            .ok();
                                        }
                                        _ => {
                                            // Invalid file type
                                            eprintln!("Invalid file type. Please select an .mp4 or .mov file.");
                                        }
                                    }
                                }
                            }
                        })
                        .detach();
                    })
                    .child("Open File"),
            )
    }
}

/// Video player window that displays the video
struct VideoPlayerWindow;

impl Render for VideoPlayerWindow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        // Full window for video display - GStreamer will render directly to this window
        div().flex().bg(rgb(0x000000)).size_full()
    }
}

/// Controls window with play/pause/stop buttons and video scrubber
struct ControlsWindow {
    slider_state: Entity<SliderState>,
    current_position: f32,
    duration: f32,
}

impl ControlsWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        let slider_state = cx.new(|_cx| {
            SliderState::new()
                .min(0.0)
                .max(36000.0) // Max 10 hours (will be updated once duration is known)
                .step(0.1)
                .default_value(0.0)
        });

        // Subscribe to slider events
        cx.subscribe(&slider_state, |_this, _, event: &SliderEvent, cx| {
            let SliderEvent::Change(value) = event;
            let position_secs = value.end();

            // Seek the video
            let app_state = cx.global::<AppState>();
            let video_player = app_state.video_player.clone();

            if let Ok(player) = video_player.lock() {
                use gstreamer::ClockTime;
                let nanos = (position_secs * 1_000_000_000.0) as u64;
                let clock_time = ClockTime::from_nseconds(nanos);
                if let Err(e) = player.seek(clock_time) {
                    eprintln!("Failed to seek: {}", e);
                }
            }
        })
        .detach();

        Self {
            slider_state,
            current_position: 0.0,
            duration: 0.0,
        }
    }

    fn update_position_from_player(&mut self, cx: &mut Context<Self>) {
        let app_state = cx.global::<AppState>();
        let video_player = app_state.video_player.clone();

        if let Ok(player) = video_player.lock() {
            if let Some((position, duration)) = player.get_position_duration() {
                self.current_position = position.seconds() as f32;
                self.duration = duration.seconds() as f32;
            }
        }
    }

    fn format_time(seconds: f32) -> String {
        let total_secs = seconds as u64;
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{:02}:{:02}", mins, secs)
    }
}

impl Render for ControlsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        cx.on_next_frame(window, |t, _window, cx| {
            // Update from video player and request next frame
            t.update_position_from_player(cx);

            // Request another render on next frame to create continuous updates
            cx.notify();
        });

        // Update slider state to match current video position and duration
        if self.duration > 0.0 {
            // Update max if duration is known
            let current_max = self.slider_state.read(cx).get_max();
            if (current_max - self.duration).abs() > 0.1 {
                // Update the slider's max value to match the video duration
                self.slider_state.update(cx, |state, cx| {
                    state.set_max(self.duration, window, cx);
                });
            }
            // Update the position
            self.slider_state.update(cx, |state, cx| {
                state.set_value(SliderValue::Single(self.current_position), window, cx);
            });
        }

        let current_time = self.current_position;
        let duration = if self.duration > 0.0 {
            self.duration
        } else {
            100.0
        };

        div()
            .flex()
            .flex_col()
            .bg(rgb(0x1b5e20))
            .size_full()
            .p_4()
            .gap_3()
            // Slider and time display section
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .w_full()
                    // Time display
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .w_full()
                            .text_sm()
                            .text_color(rgb(0xffffff))
                            .child(Self::format_time(current_time))
                            .child(Self::format_time(duration)),
                    )
                    // Slider
                    .child(Slider::new(&self.slider_state).horizontal()),
            )
            // Button controls section
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_4()
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .bg(rgb(0x388e3c))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xffffff))
                            .hover(|style| style.bg(rgb(0x4caf50)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_, _, _, cx| {
                                    let app_state = cx.global::<AppState>();
                                    let video_player = app_state.video_player.clone();
                                    if let Ok(player) = video_player.lock() {
                                        if let Err(e) = player.play() {
                                            eprintln!("Failed to play: {}", e);
                                        }
                                    }
                                }),
                            )
                            .child("Play"),
                    )
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .bg(rgb(0x388e3c))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xffffff))
                            .hover(|style| style.bg(rgb(0x4caf50)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_, _, _, cx| {
                                    let app_state = cx.global::<AppState>();
                                    let video_player = app_state.video_player.clone();
                                    if let Ok(player) = video_player.lock() {
                                        if let Err(e) = player.pause() {
                                            eprintln!("Failed to pause: {}", e);
                                        }
                                    }
                                }),
                            )
                            .child("Pause"),
                    )
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .bg(rgb(0x388e3c))
                            .rounded_md()
                            .cursor_pointer()
                            .text_color(rgb(0xffffff))
                            .hover(|style| style.bg(rgb(0x4caf50)))
                            .on_mouse_down(
                                gpui::MouseButton::Left,
                                cx.listener(|_, _, _, cx| {
                                    let app_state = cx.global::<AppState>();
                                    let video_player = app_state.video_player.clone();
                                    if let Ok(player) = video_player.lock() {
                                        if let Err(e) = player.stop() {
                                            eprintln!("Failed to stop: {}", e);
                                        }
                                    }
                                }),
                            )
                            .child("Stop"),
                    ),
            )
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

struct AppState {
    file_path: Option<String>,
    initial_window: Option<AnyWindowHandle>,
    video_window: Option<AnyWindowHandle>,
    controls_window: Option<AnyWindowHandle>,
    video_player: Arc<Mutex<video_player::VideoPlayer>>,
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            initial_window: None,
            video_window: None,
            controls_window: None,
            video_player: Arc::new(Mutex::new(video_player::VideoPlayer::new())),
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
fn create_video_windows(cx: &mut App, path_string: String, path_clone: String) {
    // Close existing windows by calling remove_window()
    println!("Closing existing windows");

    // Get handles before clearing state
    let app_state = cx.global::<AppState>();
    let initial_window = app_state.initial_window;
    let video_window = app_state.video_window;
    let controls_window = app_state.controls_window;

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

    // Clear the handles from state
    cx.update_global::<AppState, _>(|state, _| {
        state.initial_window = None;
        state.video_window = None;
        state.controls_window = None;
    });

    // Extract just the file name from the path for the window title
    let file_name = std::path::Path::new(&path_string)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Video Player")
        .to_string();

    // Create the video player window (closable)
    let video_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds::centered(
            None,
            gpui::size(px(1280.0), px(720.0)),
            cx,
        ))),
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

    // Create the controls window (not closable, no title)
    let controls_window_options = WindowOptions {
        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
            origin: gpui::point(px(100.0), px(100.0)),
            size: gpui::size(px(500.0), px(120.0)),
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

    // Update AppState with new windows and file path
    cx.update_global::<AppState, _>(|state, _| {
        state.video_window = Some(video_window.into());
        state.controls_window = Some(controls_window.into());
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
                            create_video_windows(cx, path_string, path_clone);
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
