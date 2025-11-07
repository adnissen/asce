//! ASVE - Video Editor with GPUI
//!
//! ## Native Window Handle Access
//!
//! This application stores a `AnyWindowHandle` in the global `AppState`, which can be used
//! to access the underlying native window handle for integration with native APIs.
//!
//! ### Usage Example:
//!
//! To access the raw window handle (for APIs like wgpu, FFmpeg hardware acceleration, etc.):
//!
//! ```rust
//! // From within an action or async task with access to App context:
//! cx.update(|cx| {
//!     let app_state = cx.global::<AppState>();
//!     if let Some(window_handle) = app_state.window_handle() {
//!         cx.update_window(window_handle, |_window, _cx| {
//!             // The window's underlying PlatformWindow implements HasWindowHandle
//!             // from the raw-window-handle crate, allowing you to pass it to
//!             // native APIs that need platform-specific window handles
//!         }).ok();
//!     }
//! }).ok();
//! ```
//!
//! The `raw-window-handle` crate is included as a dependency, which provides
//! cross-platform window handle abstraction used by graphics APIs like wgpu.

use gpui::{
    AnyWindowHandle, App, Application, Context, Global, Menu, MenuItem, PathPromptOptions,
    SystemMenuType, Window, WindowOptions, actions, div, prelude::*, rgb,
};

struct SetMenus;

impl Render for SetMenus {
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
            .open_window(WindowOptions::default(), |_window, cx| {
                cx.new(|_| SetMenus {})
            })
            .unwrap();

        // Store the window handle in AppState for later use
        // This AnyWindowHandle can be used to access the window from anywhere in the app
        cx.update_global::<AppState, _>(|state, _| {
            state.window_handle = Some(window.into());
        });

        // Log information about the window handle (for demonstration)
        println!("Window created with handle: {:?}", window);
    });
}

struct AppState {
    file_path: Option<String>,
    window_handle: Option<AnyWindowHandle>,
}

impl AppState {
    fn new() -> Self {
        Self {
            file_path: None,
            window_handle: None,
        }
    }

    /// Get the stored window handle for use with GPUI APIs
    ///
    /// This can be used with cx.update_window() to access window-specific functionality.
    /// To get raw platform handles (for passing to native APIs like wgpu, ffmpeg, etc.),
    /// you need to access the window through GPUI's context system.
    ///
    /// Example usage:
    /// ```
    /// if let Some(handle) = app_state.window_handle() {
    ///     cx.update_window(handle, |_window, cx| {
    ///         // Access window here
    ///         // The underlying PlatformWindow implements HasWindowHandle trait
    ///         // which can provide raw window handles for native APIs
    ///     }).ok();
    /// }
    /// ```
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
                        cx.update(|cx| {
                            let app_state = cx.global_mut::<AppState>();
                            app_state.file_path = Some(path_string);
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
