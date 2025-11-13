//! Video Player Module using GStreamer
//!
//! This module provides video playback functionality using GStreamer with
//! VideoOverlay for rendering to a native macOS NSView window.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video::prelude::*;
use std::fmt;

/// Errors that can occur during video playback
#[derive(Debug)]
pub enum VideoPlayerError {
    GStreamerInit(gst::glib::Error),
    PipelineCreation(gst::glib::BoolError),
    StateChange(gst::StateChangeError),
    NoWindowHandle,
    InvalidFilePath,
    BusError,
}

impl fmt::Display for VideoPlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GStreamerInit(e) => write!(f, "GStreamer initialization failed: {}", e),
            Self::PipelineCreation(e) => write!(f, "Pipeline creation failed: {}", e),
            Self::StateChange(e) => write!(f, "State change failed: {}", e),
            Self::NoWindowHandle => write!(f, "No window handle available"),
            Self::InvalidFilePath => write!(f, "Invalid file path"),
            Self::BusError => write!(f, "Bus error"),
        }
    }
}

impl std::error::Error for VideoPlayerError {}

/// Video player using GStreamer with VideoOverlay support
pub struct VideoPlayer {
    pipeline: Option<gst::Element>,
    ns_view_handle: Option<usize>,
    render_rect: Option<(i32, i32, i32, i32)>, // x, y, width, height
    bus_watch_guard: Option<gst::bus::BusWatchGuard>,
}

impl VideoPlayer {
    /// Create a new VideoPlayer instance
    pub fn new() -> Self {
        Self {
            pipeline: None,
            ns_view_handle: None,
            render_rect: Some((0, 0, 800, 600)),
            bus_watch_guard: None,
        }
    }

    /// Set the native window handle (NSView pointer) for video rendering
    ///
    /// This must be called before loading a file to enable video overlay.
    /// The handle should be the raw NSView pointer from macOS.
    pub fn set_window_handle(&mut self, handle: usize) {
        self.ns_view_handle = Some(handle);
        println!("VideoPlayer: Window handle set to 0x{:x}", handle);
    }

    /// Load a video file and prepare for playback
    ///
    /// This creates a GStreamer pipeline with playbin and sets up the
    /// VideoOverlay to render to the window handle if one was set.
    pub fn load_file(&mut self, file_path: &str) -> Result<(), VideoPlayerError> {
        // Validate file exists
        if !std::path::Path::new(file_path).exists() {
            return Err(VideoPlayerError::InvalidFilePath);
        }

        println!("VideoPlayer: Loading file: {}", file_path);

        // Create playbin pipeline
        let pipeline = gst::ElementFactory::make("playbin")
            .build()
            .map_err(VideoPlayerError::PipelineCreation)?;

        // Set the URI
        let uri = format!("file://{}", file_path);
        pipeline.set_property("uri", &uri);

        // Set up VideoOverlay if we have a window handle
        if let Some(handle) = self.ns_view_handle {
            self.setup_video_overlay(&pipeline, handle);
        } else {
            println!("VideoPlayer: Warning - No window handle set, video may not display");
        }

        // Store the pipeline
        self.pipeline = Some(pipeline);

        println!("VideoPlayer: File loaded successfully");
        Ok(())
    }

    /// Set up VideoOverlay to render video to the native window
    ///
    /// This uses a sync bus handler to catch the prepare-window-handle message
    /// and set the window handle on the video sink.
    fn setup_video_overlay(&self, pipeline: &gst::Element, window_handle: usize) {
        println!(
            "VideoPlayer: Setting up VideoOverlay with handle 0x{:x}",
            window_handle
        );

        let bus = pipeline.bus().expect("Pipeline should have a bus");

        bus.set_sync_handler(move |_bus, msg| {
            if let Some(msg_structure) = msg.structure() {
                if msg_structure.has_name("prepare-window-handle") {
                    println!("VideoPlayer: Received prepare-window-handle message");

                    if let Some(element) = msg.src() {
                        // Try to cast to VideoOverlay (clone first since dynamic_cast takes ownership)
                        if let Ok(overlay) = element
                            .clone()
                            .dynamic_cast::<gstreamer_video::VideoOverlay>()
                        {
                            println!("VideoPlayer: Setting window handle on video overlay");
                            unsafe {
                                overlay.set_window_handle(window_handle);
                            }
                        }
                    }

                    return gst::BusSyncReply::Drop;
                }
            }
            gst::BusSyncReply::Pass
        });
    }

    /// Start watching the bus for messages using GLib main loop
    ///
    /// This sets up a message handler that will be called automatically
    /// when messages arrive on the bus. Returns a Result to indicate success.
    pub fn start_message_watch(&mut self) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            let bus = pipeline.bus().ok_or(VideoPlayerError::BusError)?;

            // Clone the pipeline for use in the closure
            let pipeline_weak = pipeline.downgrade();

            let guard = bus
                .add_watch(move |_bus, msg| {
                    use gst::MessageView;

                    // Get a strong reference to the pipeline if it still exists
                    let pipeline = match pipeline_weak.upgrade() {
                        Some(p) => p,
                        None => return gst::glib::ControlFlow::Break,
                    };

                    match msg.view() {
                        MessageView::Eos(..) => {
                            println!("VideoPlayer: End of stream reached");
                            // Could loop or stop here
                            let _ = pipeline.set_state(gst::State::Null);
                        }
                        MessageView::Error(err) => {
                            eprintln!(
                                "VideoPlayer Error from {:?}: {} ({:?})",
                                err.src().map(|s| s.path_string()),
                                err.error(),
                                err.debug()
                            );
                            let _ = pipeline.set_state(gst::State::Null);
                        }
                        MessageView::Warning(warning) => {
                            eprintln!(
                                "VideoPlayer Warning from {:?}: {} ({:?})",
                                warning.src().map(|s| s.path_string()),
                                warning.error(),
                                warning.debug()
                            );
                        }
                        MessageView::StateChanged(state_changed) => {
                            // Only handle state changes from the pipeline itself, not child elements
                            if msg.src().map(|s| s == &pipeline).unwrap_or(false) {
                                println!(
                                    "VideoPlayer: Pipeline state changed from {:?} to {:?}",
                                    state_changed.old(),
                                    state_changed.current()
                                );
                            }
                        }
                        MessageView::AsyncDone(..) => {
                            println!("VideoPlayer: Async state change completed");
                        }
                        _ => {}
                    }

                    gst::glib::ControlFlow::Continue
                })
                .map_err(|_| VideoPlayerError::BusError)?;

            // Store the guard to prevent it from being dropped
            self.bus_watch_guard = Some(guard);

            println!("VideoPlayer: Bus watch started");
            Ok(())
        } else {
            Err(VideoPlayerError::NoWindowHandle)
        }
    }

    /// Start playback
    pub fn play(&self) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            println!("VideoPlayer: Starting playback");

            pipeline
                .set_state(gst::State::Playing)
                .map_err(VideoPlayerError::StateChange)?;

            println!("VideoPlayer: Playback started successfully");

            Ok(())
        } else {
            eprintln!("VideoPlayer: No pipeline loaded");
            Err(VideoPlayerError::NoWindowHandle)
        }
    }

    /// Pause playback
    pub fn pause(&self) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            println!("VideoPlayer: Pausing playback");
            pipeline
                .set_state(gst::State::Paused)
                .map_err(VideoPlayerError::StateChange)?;
            Ok(())
        } else {
            eprintln!("VideoPlayer: No pipeline loaded");
            Err(VideoPlayerError::NoWindowHandle)
        }
    }

    /// Stop playback and reset to beginning
    pub fn stop(&self) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            println!("VideoPlayer: Stopping playback");
            pipeline
                .set_state(gst::State::Null)
                .map_err(VideoPlayerError::StateChange)?;
            Ok(())
        } else {
            Ok(()) // Already stopped
        }
    }

    /// Get the current playback position and duration
    pub fn get_position_duration(&self) -> Option<(gst::ClockTime, gst::ClockTime)> {
        if let Some(ref pipeline) = self.pipeline {
            let position = pipeline.query_position::<gst::ClockTime>()?;
            let duration = pipeline.query_duration::<gst::ClockTime>()?;
            Some((position, duration))
        } else {
            None
        }
    }

    /// Check if the video is currently playing
    pub fn is_playing(&self) -> bool {
        if let Some(ref pipeline) = self.pipeline {
            let (_, state, _) = pipeline.state(gst::ClockTime::from_mseconds(0));
            return state == gst::State::Playing;
        }
        false
    }

    /// Seek to a specific position
    pub fn seek(&self, position: gst::ClockTime) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            println!("VideoPlayer: Seeking to {:?}", position);
            // Use ACCURATE flag for precise seeking to exact timestamps
            // This is important for subtitle synchronization
            pipeline
                .seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, position)
                .map_err(|_| {
                    eprintln!("VideoPlayer: Seek failed");
                    VideoPlayerError::InvalidFilePath
                })?;
            Ok(())
        } else {
            eprintln!("VideoPlayer: No pipeline loaded");
            Err(VideoPlayerError::NoWindowHandle)
        }
    }

    /// Enable or disable subtitle display
    ///
    /// When enabled, sets current-text to the specified track.
    /// When disabled, sets current-text to -1.
    pub fn set_subtitle_display(
        &self,
        enabled: bool,
        track_index: Option<i32>,
    ) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            if enabled {
                if let Some(index) = track_index {
                    println!(
                        "VideoPlayer: Enabling subtitle display with track {}",
                        index
                    );
                    pipeline.set_property("current-text", index);
                } else {
                    println!("VideoPlayer: Cannot enable subtitles - no track specified");
                    return Err(VideoPlayerError::InvalidFilePath);
                }
            } else {
                println!("VideoPlayer: Disabling subtitle display");
                pipeline.set_property("current-text", -1);
            }
            Ok(())
        } else {
            eprintln!("VideoPlayer: No pipeline loaded");
            Err(VideoPlayerError::NoWindowHandle)
        }
    }

    /// Set the current subtitle track to display
    pub fn set_subtitle_track(&self, track_index: i32) -> Result<(), VideoPlayerError> {
        if let Some(ref pipeline) = self.pipeline {
            println!("VideoPlayer: Setting subtitle track to {}", track_index);
            pipeline.set_property("current-text", track_index);
            Ok(())
        } else {
            eprintln!("VideoPlayer: No pipeline loaded");
            Err(VideoPlayerError::NoWindowHandle)
        }
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        if let Some(ref pipeline) = self.pipeline {
            println!("VideoPlayer: Cleaning up pipeline");
            let _ = pipeline.set_state(gst::State::Null);

            // Give GStreamer a moment to cleanup
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
}

/// Initialize GStreamer and start the GLib main loop
///
/// This should be called once at application startup before creating any
/// VideoPlayer instances. It initializes GStreamer and starts a background
/// thread running the GLib main loop to handle bus messages.
pub fn init() -> Result<(), VideoPlayerError> {
    gst::init().map_err(VideoPlayerError::GStreamerInit)?;

    println!("GStreamer initialized successfully");
    println!("GStreamer version: {}", gst::version_string());

    // Check for required plugins
    let required_plugins = [
        "playbin",
        "qtdemux",
        "glimagesink",
        "videoscale",
        "videoconvert",
    ];
    let mut missing_plugins = Vec::new();

    for plugin_name in &required_plugins {
        if gst::ElementFactory::find(plugin_name).is_none() {
            missing_plugins.push(*plugin_name);
        }
    }

    if !missing_plugins.is_empty() {
        eprintln!("Warning: Missing GStreamer plugins: {:?}", missing_plugins);
        eprintln!(
            "Some features may not work. Install with: brew install gstreamer gst-plugins-base gst-plugins-good"
        );
    }

    // Start the GLib main loop in a background thread
    // This is required for bus watches to work
    std::thread::spawn(|| {
        println!("GLib main loop thread started");
        let main_loop = gst::glib::MainLoop::new(None, false);
        main_loop.run();
    });

    Ok(())
}
