//! Video Player Module using libmpv
//!
//! This module provides video playback functionality using libmpv with libplacebo rendering.

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

#[cfg(target_os = "macos")]
use cocoa::appkit::{NSOpenGLContext, NSOpenGLPixelFormat};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use cocoa::foundation::NSAutoreleasePool;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::Gdi::*;
#[cfg(target_os = "windows")]
use windows::Win32::Graphics::OpenGL::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

// Include generated mpv bindings
#[allow(non_upper_case_globals)]
#[allow(non_camel_case_types)]
#[allow(non_snake_case)]
#[allow(dead_code)]
#[allow(unsafe_op_in_unsafe_fn)]
#[allow(unused_imports)]
#[allow(improper_ctypes)]
mod mpv_sys {
    #![allow(unsafe_op_in_unsafe_fn)]
    include!(concat!(env!("OUT_DIR"), "/mpv_bindings.rs"));
}

use mpv_sys::*;

/// Wrapper for mpv_handle that implements Send
/// mpv's documentation states the handle is thread-safe
struct SendMpvHandle(*mut mpv_handle);
unsafe impl Send for SendMpvHandle {}
unsafe impl Sync for SendMpvHandle {}

/// Wrapper for mpv_render_context that implements Send
struct SendMpvRenderContext(*mut mpv_render_context);
unsafe impl Send for SendMpvRenderContext {}
unsafe impl Sync for SendMpvRenderContext {}

// Platform-specific OpenGL context wrappers

/// Wrapper for NSOpenGLContext that implements Send (macOS)
/// Safe because we only use it from the render thread
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct SendGLContext(id);
#[cfg(target_os = "macos")]
unsafe impl Send for SendGLContext {}
#[cfg(target_os = "macos")]
unsafe impl Sync for SendGLContext {}

/// Wrapper for HWND and HGLRC that implements Send (Windows)
/// Safe because we only use it from the render thread
#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
struct SendGLContext {
    hwnd: isize,
    hdc: isize,
    hglrc: isize,
}
#[cfg(target_os = "windows")]
unsafe impl Send for SendGLContext {}
#[cfg(target_os = "windows")]
unsafe impl Sync for SendGLContext {}

/// Errors that can occur during video playback
#[derive(Debug)]
pub enum VideoPlayerError {
    MpvError(String),
    InitializationError(String),
    InvalidFilePath,
    CommandError(String),
}

impl fmt::Display for VideoPlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MpvError(e) => write!(f, "mpv error: {}", e),
            Self::InitializationError(e) => write!(f, "Initialization error: {}", e),
            Self::InvalidFilePath => write!(f, "Invalid file path"),
            Self::CommandError(e) => write!(f, "Command error: {}", e),
        }
    }
}

impl std::error::Error for VideoPlayerError {}

/// Shared state for tracking playback status
struct PlaybackState {
    position_ns: AtomicU64,
    duration_ns: AtomicU64,
    paused: AtomicBool,
}

impl PlaybackState {
    fn new() -> Self {
        Self {
            position_ns: AtomicU64::new(0),
            duration_ns: AtomicU64::new(0),
            paused: AtomicBool::new(true),
        }
    }

    fn set_position(&self, ns: u64) {
        self.position_ns.store(ns, Ordering::SeqCst);
    }

    fn get_position(&self) -> u64 {
        self.position_ns.load(Ordering::SeqCst)
    }

    fn set_duration(&self, ns: u64) {
        self.duration_ns.store(ns, Ordering::SeqCst);
    }

    fn get_duration(&self) -> u64 {
        self.duration_ns.load(Ordering::SeqCst)
    }

    fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
    }

    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }
}

/// Video player using libmpv
pub struct VideoPlayer {
    mpv_handle: SendMpvHandle,
    render_context: Option<SendMpvRenderContext>,
    gl_context: Option<SendGLContext>,
    state: Arc<PlaybackState>,
    event_thread: Option<JoinHandle<()>>,
    render_thread: Option<JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    needs_render: Arc<AtomicBool>,
    // FBO rendering support
    fbo_id: Option<u32>,
    texture_id: Option<u32>,
    video_width: u32,
    video_height: u32,
    frame_buffer: Arc<Mutex<Vec<u8>>>,
}

impl VideoPlayer {
    /// Create a new VideoPlayer instance
    pub fn new() -> Self {
        unsafe {
            let handle = mpv_create();
            if handle.is_null() {
                panic!("Failed to create mpv handle");
            }

            // Set some basic options before initialization
            Self::set_option_string(handle, "vo", "libmpv"); // Use render API, don't create window
            Self::set_option_string(handle, "idle", "yes"); // Keep mpv running
            Self::set_option_string(handle, "keep-open", "yes"); // Keep file open at end

            // Default video dimensions (will be updated when window is set)
            let video_width = 960;
            let video_height = 540;
            let buffer_size = (video_width * video_height * 4) as usize; // RGBA

            Self {
                mpv_handle: SendMpvHandle(handle),
                render_context: None,
                gl_context: None,
                state: Arc::new(PlaybackState::new()),
                event_thread: None,
                render_thread: None,
                shutdown: Arc::new(AtomicBool::new(false)),
                needs_render: Arc::new(AtomicBool::new(false)),
                fbo_id: None,
                texture_id: None,
                video_width,
                video_height,
                frame_buffer: Arc::new(Mutex::new(vec![0u8; buffer_size])),
            }
        }
    }

    /// Helper to set string options
    unsafe fn set_option_string(handle: *mut mpv_handle, name: &str, value: &str) {
        let name_c = CString::new(name).unwrap();
        let value_c = CString::new(value).unwrap();
        unsafe {
            mpv_set_option_string(handle, name_c.as_ptr(), value_c.as_ptr());
        }
    }

    /// Initialize mpv (must be called after setting initial options)
    fn initialize(&mut self) -> Result<(), VideoPlayerError> {
        unsafe {
            let ret = mpv_initialize(self.mpv_handle.0);
            if ret < 0 {
                return Err(VideoPlayerError::InitializationError(Self::error_string(
                    ret,
                )));
            }

            // Start observing properties
            self.observe_property("time-pos", mpv_format_MPV_FORMAT_DOUBLE)?;
            self.observe_property("duration", mpv_format_MPV_FORMAT_DOUBLE)?;
            self.observe_property("pause", mpv_format_MPV_FORMAT_FLAG)?;

            // Start event loop thread
            let handle = SendMpvHandle(self.mpv_handle.0);
            let state = Arc::clone(&self.state);
            let shutdown = Arc::clone(&self.shutdown);

            let event_thread = thread::spawn(move || {
                Self::event_loop(handle, state, shutdown);
            });

            self.event_thread = Some(event_thread);

            println!("VideoPlayer: mpv initialized successfully");
            Ok(())
        }
    }

    /// Observe a property for changes
    fn observe_property(&self, name: &str, format: mpv_format) -> Result<(), VideoPlayerError> {
        unsafe {
            let name_c = CString::new(name).unwrap();
            let ret = mpv_observe_property(
                self.mpv_handle.0,
                0, // reply_userdata
                name_c.as_ptr(),
                format,
            );
            if ret < 0 {
                return Err(VideoPlayerError::MpvError(Self::error_string(ret)));
            }
            Ok(())
        }
    }

    /// Event loop that runs in a separate thread
    fn event_loop(handle: SendMpvHandle, state: Arc<PlaybackState>, shutdown: Arc<AtomicBool>) {
        unsafe {
            loop {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }

                let event = mpv_wait_event(handle.0, 0.1); // 100ms timeout
                if event.is_null() {
                    continue;
                }

                let event_ref = &*event;

                match event_ref.event_id {
                    mpv_event_id_MPV_EVENT_PROPERTY_CHANGE => {
                        if !event_ref.data.is_null() {
                            let prop = &*(event_ref.data as *const mpv_event_property);
                            let name = CStr::from_ptr(prop.name).to_str().unwrap_or("");

                            match name {
                                "time-pos" => {
                                    if prop.format == mpv_format_MPV_FORMAT_DOUBLE
                                        && !prop.data.is_null()
                                    {
                                        let pos_secs = *(prop.data as *const f64);
                                        state.set_position((pos_secs * 1_000_000_000.0) as u64);
                                    }
                                }
                                "duration" => {
                                    if prop.format == mpv_format_MPV_FORMAT_DOUBLE
                                        && !prop.data.is_null()
                                    {
                                        let dur_secs = *(prop.data as *const f64);
                                        state.set_duration((dur_secs * 1_000_000_000.0) as u64);
                                    }
                                }
                                "pause" => {
                                    if prop.format == mpv_format_MPV_FORMAT_FLAG
                                        && !prop.data.is_null()
                                    {
                                        let paused = *(prop.data as *const c_int) != 0;
                                        state.set_paused(paused);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    mpv_event_id_MPV_EVENT_SHUTDOWN => {
                        println!("VideoPlayer: Received shutdown event");
                        break;
                    }
                    mpv_event_id_MPV_EVENT_END_FILE => {
                        println!("VideoPlayer: End of file");
                    }
                    _ => {}
                }
            }
        }
    }

    /// Set the native window handle and create OpenGL context (macOS)
    #[cfg(target_os = "macos")]
    pub fn set_window_handle(&mut self, handle: usize) {
        unsafe {
            println!(
                "VideoPlayer: Setting up OpenGL context for NSView: 0x{:x}",
                handle
            );

            let _pool = NSAutoreleasePool::new(nil);
            let ns_view = handle as id;

            // Create OpenGL pixel format
            use cocoa::appkit::{
                NSOpenGLPFAAccelerated, NSOpenGLPFAAlphaSize, NSOpenGLPFAColorSize,
                NSOpenGLPFADepthSize, NSOpenGLPFADoubleBuffer, NSOpenGLPFAOpenGLProfile,
                NSOpenGLProfileVersion3_2Core,
            };
            let pixel_format_attrs: &[u32] = &[
                NSOpenGLPFADoubleBuffer as u32,
                NSOpenGLPFAAccelerated as u32,
                NSOpenGLPFAOpenGLProfile as u32,
                NSOpenGLProfileVersion3_2Core as u32,
                NSOpenGLPFAColorSize as u32,
                24,
                NSOpenGLPFAAlphaSize as u32,
                8,
                NSOpenGLPFADepthSize as u32,
                24,
                0,
            ];

            let pixel_format =
                NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(pixel_format_attrs);
            if pixel_format == nil {
                eprintln!("Failed to create NSOpenGLPixelFormat");
                return;
            }

            // Create OpenGL context
            let gl_context =
                NSOpenGLContext::alloc(nil).initWithFormat_shareContext_(pixel_format, nil);
            if gl_context == nil {
                eprintln!("Failed to create NSOpenGLContext");
                return;
            }

            // Set the view for the context
            let () = msg_send![gl_context, setView: ns_view];
            let () = msg_send![gl_context, makeCurrentContext];

            self.gl_context = Some(SendGLContext(gl_context));

            println!("VideoPlayer: OpenGL context created successfully");

            // Load OpenGL function pointers
            gl::load_with(|name| {
                let name_c = CString::new(name).unwrap();
                let symbol = core_foundation::string::CFString::new(name);
                let framework = core_foundation::string::CFString::from_static_string("com.apple.opengl");
                let bundle = core_foundation::bundle::CFBundleGetBundleWithIdentifier(framework.as_concrete_TypeRef());
                if bundle.is_null() {
                    return std::ptr::null();
                }
                core_foundation::bundle::CFBundleGetFunctionPointerForName(bundle, symbol.as_concrete_TypeRef()) as *const std::ffi::c_void
            });

            println!("VideoPlayer: OpenGL functions loaded");

            // Create FBO and texture for off-screen rendering
            self.create_fbo();

            // Don't create render context here - wait until mpv is initialized in load_file()
        }
    }

    /// Set the native window handle and create OpenGL context (Windows)
    #[cfg(target_os = "windows")]
    pub fn set_window_handle(&mut self, handle: usize) {
        unsafe {
            let hwnd = HWND(handle as isize as *mut _);
            println!("VideoPlayer: Setting up OpenGL 3.2 context for HWND: {:?}", hwnd);

            // Get device context
            let hdc = GetDC(Some(hwnd));
            if hdc.0.is_null() {
                eprintln!("Failed to get device context");
                return;
            }

            // Set up pixel format descriptor
            let pfd = PIXELFORMATDESCRIPTOR {
                nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
                nVersion: 1,
                dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
                iPixelType: PFD_TYPE_RGBA,
                cColorBits: 32,
                cRedBits: 0,
                cRedShift: 0,
                cGreenBits: 0,
                cGreenShift: 0,
                cBlueBits: 0,
                cBlueShift: 0,
                cAlphaBits: 8,
                cAlphaShift: 0,
                cAccumBits: 0,
                cAccumRedBits: 0,
                cAccumGreenBits: 0,
                cAccumBlueBits: 0,
                cAccumAlphaBits: 0,
                cDepthBits: 24,
                cStencilBits: 8,
                cAuxBuffers: 0,
                iLayerType: PFD_MAIN_PLANE.0 as u8,
                bReserved: 0,
                dwLayerMask: 0,
                dwVisibleMask: 0,
                dwDamageMask: 0,
            };

            let pixel_format = ChoosePixelFormat(hdc, &pfd);
            if pixel_format == 0 {
                eprintln!("Failed to choose pixel format");
                ReleaseDC(Some(hwnd), hdc);
                return;
            }

            if SetPixelFormat(hdc, pixel_format, &pfd).is_err() {
                eprintln!("Failed to set pixel format");
                ReleaseDC(Some(hwnd), hdc);
                return;
            }

            // Create a temporary legacy context to get extension function pointers
            let temp_context = match wglCreateContext(hdc) {
                Ok(ctx) => ctx,
                Err(_) => {
                    eprintln!("Failed to create temporary WGL context");
                    ReleaseDC(Some(hwnd), hdc);
                    return;
                }
            };

            if wglMakeCurrent(hdc, temp_context).is_err() {
                eprintln!("Failed to make temporary context current");
                let _ = wglDeleteContext(temp_context);
                ReleaseDC(Some(hwnd), hdc);
                return;
            }

            // Get wglCreateContextAttribsARB function pointer
            type WglCreateContextAttribsARB = unsafe extern "system" fn(
                hdc: HDC,
                hsharecontext: HGLRC,
                attriblist: *const i32,
            ) -> HGLRC;

            let wgl_create_context_attribs: Option<WglCreateContextAttribsARB> = {
                let proc_name = b"wglCreateContextAttribsARB\0";
                let proc_addr = wglGetProcAddress(windows::core::PCSTR(proc_name.as_ptr()));
                if proc_addr.is_none() {
                    eprintln!("wglCreateContextAttribsARB not available - using legacy context");
                    None
                } else {
                    println!("wglCreateContextAttribsARB found, creating OpenGL 3.2 context");
                    Some(std::mem::transmute(proc_addr.unwrap()))
                }
            };

            // Try to create OpenGL 3.2 Core Profile context if available
            let hglrc = if let Some(create_fn) = wgl_create_context_attribs {
                const WGL_CONTEXT_MAJOR_VERSION_ARB: i32 = 0x2091;
                const WGL_CONTEXT_MINOR_VERSION_ARB: i32 = 0x2092;
                const WGL_CONTEXT_PROFILE_MASK_ARB: i32 = 0x9126;
                const WGL_CONTEXT_CORE_PROFILE_BIT_ARB: i32 = 0x00000001;

                let attribs = [
                    WGL_CONTEXT_MAJOR_VERSION_ARB, 3,
                    WGL_CONTEXT_MINOR_VERSION_ARB, 2,
                    WGL_CONTEXT_PROFILE_MASK_ARB, WGL_CONTEXT_CORE_PROFILE_BIT_ARB,
                    0, // Null terminator
                ];

                let ctx = create_fn(hdc, HGLRC(std::ptr::null_mut()), attribs.as_ptr());

                if ctx.0.is_null() {
                    eprintln!("Failed to create OpenGL 3.2 context, keeping temp context");
                    temp_context
                } else {
                    println!("OpenGL 3.2 Core context created successfully");
                    // Clean up temporary context
                    let _ = wglMakeCurrent(HDC(std::ptr::null_mut()), HGLRC(std::ptr::null_mut()));
                    let _ = wglDeleteContext(temp_context);
                    ctx
                }
            } else {
                // Fall back to legacy context
                println!("Using legacy OpenGL context");
                temp_context
            };

            // Make the context current
            if wglMakeCurrent(hdc, hglrc).is_err() {
                eprintln!("Failed to make OpenGL context current");
                let _ = wglDeleteContext(hglrc);
                ReleaseDC(Some(hwnd), hdc);
                return;
            }

            // Query and print OpenGL version
            let version_ptr = windows::Win32::Graphics::OpenGL::glGetString(windows::Win32::Graphics::OpenGL::GL_VERSION);
            if !version_ptr.is_null() {
                let version = std::ffi::CStr::from_ptr(version_ptr as *const i8);
                println!("OpenGL version: {:?}", version);
            } else {
                eprintln!("Failed to query OpenGL version");
            }

            // Query renderer info
            let renderer_ptr = windows::Win32::Graphics::OpenGL::glGetString(windows::Win32::Graphics::OpenGL::GL_RENDERER);
            if !renderer_ptr.is_null() {
                let renderer = std::ffi::CStr::from_ptr(renderer_ptr as *const i8);
                println!("OpenGL renderer: {:?}", renderer);
            }

            // Load OpenGL function pointers before releasing context
            gl::load_with(|name| {
                let name_c = CString::new(name).unwrap();
                // Try wglGetProcAddress first (for extensions)
                if let Some(proc) = wglGetProcAddress(windows::core::PCSTR(name_c.as_ptr() as *const u8)) {
                    return proc as *const std::ffi::c_void;
                }
                // Fall back to GetProcAddress from opengl32.dll (for core functions)
                if let Ok(opengl_module) = windows::Win32::System::LibraryLoader::GetModuleHandleA(windows::core::s!("opengl32.dll")) {
                    if let Some(proc) = windows::Win32::System::LibraryLoader::GetProcAddress(opengl_module, windows::core::PCSTR(name_c.as_ptr() as *const u8)) {
                        return proc as *const std::ffi::c_void;
                    }
                }
                std::ptr::null()
            });

            println!("VideoPlayer: OpenGL functions loaded");

            // Create FBO and texture for off-screen rendering
            self.create_fbo();

            // CRITICAL: Release the context from the main thread so the render thread can use it
            // OpenGL contexts can only be current on one thread at a time
            if let Err(e) = wglMakeCurrent(HDC(std::ptr::null_mut()), HGLRC(std::ptr::null_mut())) {
                eprintln!("Warning: Failed to release OpenGL context from main thread: {:?}", e);
            } else {
                println!("OpenGL context released from main thread");
            }

            self.gl_context = Some(SendGLContext {
                hwnd: hwnd.0 as isize,
                hdc: hdc.0 as isize,
                hglrc: hglrc.0 as isize,
            });

            println!("VideoPlayer: OpenGL context ready for mpv");

            // Don't create render context here - wait until mpv is initialized in load_file()
        }
    }

    /// Create FBO and texture for off-screen rendering
    fn create_fbo(&mut self) {
        unsafe {
            println!("VideoPlayer: Creating FBO for off-screen rendering ({}x{})",
                self.video_width, self.video_height);

            // Generate framebuffer
            let mut fbo: u32 = 0;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);

            // Generate texture
            let mut texture: u32 = 0;
            gl::GenTextures(1, &mut texture);
            gl::BindTexture(gl::TEXTURE_2D, texture);

            // Configure texture
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA as i32,
                self.video_width as i32,
                self.video_height as i32,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

            // Attach texture to FBO
            gl::FramebufferTexture2D(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                texture,
                0,
            );

            // Check FBO completeness
            let status = gl::CheckFramebufferStatus(gl::FRAMEBUFFER);
            if status != gl::FRAMEBUFFER_COMPLETE {
                eprintln!("VideoPlayer: FBO is not complete! Status: 0x{:X}", status);
            } else {
                println!("VideoPlayer: FBO created successfully (ID: {}, Texture: {})", fbo, texture);
            }

            // Unbind FBO
            gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

            self.fbo_id = Some(fbo);
            self.texture_id = Some(texture);
        }
    }

    /// Create mpv render context with OpenGL parameters
    fn create_render_context(&mut self) {
        unsafe {
            if self.gl_context.is_none() {
                eprintln!("Cannot create render context without GL context");
                return;
            }

            println!("VideoPlayer: Creating mpv render context");

            // Get OpenGL function pointers
            extern "C" fn get_proc_address(_ctx: *mut c_void, name: *const i8) -> *mut c_void {
                unsafe {
                    let symbol_name = CStr::from_ptr(name).to_str().unwrap();
                    let symbol_name_cstring = CString::new(symbol_name).unwrap();

                    #[cfg(target_os = "macos")]
                    {
                        use core_foundation::base::TCFType;
                        use core_foundation::bundle::{
                            CFBundleGetBundleWithIdentifier, CFBundleGetFunctionPointerForName,
                        };
                        use core_foundation::string::CFString;

                        let framework = CFString::from_static_string("com.apple.opengl");
                        let bundle = CFBundleGetBundleWithIdentifier(framework.as_concrete_TypeRef());
                        if bundle.is_null() {
                            return ptr::null_mut();
                        }

                        let symbol = CFString::new(symbol_name);
                        CFBundleGetFunctionPointerForName(bundle, symbol.as_concrete_TypeRef())
                            as *mut c_void
                    }

                    #[cfg(target_os = "windows")]
                    {
                        use windows::Win32::Graphics::OpenGL::wglGetProcAddress;
                        use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
                        use windows::core::{PCSTR, s};

                        // First try wglGetProcAddress for extension functions
                        let addr = wglGetProcAddress(PCSTR(symbol_name_cstring.as_ptr() as *const u8));
                        if let Some(proc) = addr {
                            return proc as *mut c_void;
                        }

                        // Fall back to GetProcAddress from opengl32.dll for core functions
                        if let Ok(opengl_module) = GetModuleHandleA(s!("opengl32.dll")) {
                            if let Some(proc) = GetProcAddress(opengl_module, PCSTR(symbol_name_cstring.as_ptr() as *const u8)) {
                                return proc as *mut c_void;
                            }
                        }

                        ptr::null_mut()
                    }

                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    {
                        ptr::null_mut()
                    }
                }
            }

            // Set up OpenGL init params
            let opengl_init_params = mpv_opengl_init_params {
                get_proc_address: Some(get_proc_address),
                get_proc_address_ctx: ptr::null_mut(),
            };

            let mut render_params: Vec<mpv_render_param> = vec![
                mpv_render_param {
                    type_: mpv_render_param_type_MPV_RENDER_PARAM_API_TYPE,
                    data: b"opengl\0".as_ptr() as *mut c_void,
                },
                mpv_render_param {
                    type_: mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_INIT_PARAMS,
                    data: &opengl_init_params as *const _ as *mut c_void,
                },
                mpv_render_param {
                    type_: mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                    data: ptr::null_mut(),
                },
            ];

            // On Windows, we need to make the context current temporarily for mpv to query OpenGL capabilities
            #[cfg(target_os = "windows")]
            {
                let gl_ctx = self.gl_context.as_ref().unwrap();
                let hdc = HDC(gl_ctx.hdc as *mut _);
                let hglrc = HGLRC(gl_ctx.hglrc as *mut _);

                if let Err(e) = wglMakeCurrent(hdc, hglrc) {
                    eprintln!("Failed to make context current for render context creation: {:?}", e);
                    return;
                }
                println!("Temporarily made OpenGL context current for mpv initialization");
            }

            let mut render_context: *mut mpv_render_context = ptr::null_mut();
            let ret = mpv_render_context_create(
                &mut render_context,
                self.mpv_handle.0,
                render_params.as_mut_ptr(),
            );

            // On Windows, release the context so the render thread can use it
            #[cfg(target_os = "windows")]
            {
                if let Err(e) = wglMakeCurrent(HDC(std::ptr::null_mut()), HGLRC(std::ptr::null_mut())) {
                    eprintln!("Warning: Failed to release context after render context creation: {:?}", e);
                } else {
                    println!("Released OpenGL context after mpv initialization");
                }
            }

            if ret < 0 {
                eprintln!(
                    "Failed to create render context: {}",
                    Self::error_string(ret)
                );
                return;
            }

            println!("VideoPlayer: mpv render context created successfully");
            self.render_context = Some(SendMpvRenderContext(render_context));

            // Set up update callback
            let needs_render = Arc::clone(&self.needs_render);
            extern "C" fn update_callback(ctx: *mut c_void) {
                unsafe {
                    let needs_render = &*(ctx as *const Arc<AtomicBool>);
                    needs_render.store(true, Ordering::SeqCst);
                }
            }

            let callback_ctx = Box::into_raw(Box::new(needs_render)) as *mut c_void;
            mpv_render_context_set_update_callback(
                render_context,
                Some(update_callback),
                callback_ctx,
            );

            // Start render thread
            let render_ctx = SendMpvRenderContext(render_context);
            let gl_ctx = self.gl_context.as_ref().unwrap().clone();
            let shutdown = Arc::clone(&self.shutdown);
            let needs_render = Arc::clone(&self.needs_render);
            let fbo_id = self.fbo_id.expect("FBO must be created before starting render thread");
            let frame_buffer = Arc::clone(&self.frame_buffer);
            let video_width = self.video_width;
            let video_height = self.video_height;

            let render_thread = thread::spawn(move || {
                Self::render_loop(render_ctx, gl_ctx, shutdown, needs_render, fbo_id, frame_buffer, video_width, video_height);
            });

            self.render_thread = Some(render_thread);
        }
    }

    /// Render loop that runs in a separate thread (macOS)
    #[cfg(target_os = "macos")]
    fn render_loop(
        render_ctx: SendMpvRenderContext,
        gl_context: SendGLContext,
        shutdown: Arc<AtomicBool>,
        needs_render: Arc<AtomicBool>,
        fbo_id: u32,
        frame_buffer: Arc<Mutex<Vec<u8>>>,
        video_width: u32,
        video_height: u32,
    ) {
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);

            loop {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }

                // Wait for render flag or timeout
                if !needs_render.swap(false, Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(16)); // ~60 FPS
                    continue;
                }

                // Make context current
                let () = msg_send![gl_context.0, makeCurrentContext];

                // Set up render parameters - render to our custom FBO
                let mut render_params: Vec<mpv_render_param> = vec![
                    mpv_render_param {
                        type_: mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                        data: &mpv_opengl_fbo {
                            fbo: fbo_id as i32,
                            w: video_width as i32,
                            h: video_height as i32,
                            internal_format: 0,
                        } as *const _ as *mut c_void,
                    },
                    mpv_render_param {
                        type_: mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                        data: &0i32 as *const _ as *mut c_void, // Don't flip for FBO
                    },
                    mpv_render_param {
                        type_: mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                        data: ptr::null_mut(),
                    },
                ];

                // Render to FBO
                mpv_render_context_render(render_ctx.0, render_params.as_mut_ptr());

                // Read pixels from FBO into frame buffer
                gl::BindFramebuffer(gl::FRAMEBUFFER, fbo_id);

                if let Ok(mut buffer) = frame_buffer.lock() {
                    // Use BGRA format to match video color ordering
                    gl::ReadPixels(
                        0,
                        0,
                        video_width as i32,
                        video_height as i32,
                        gl::BGRA,
                        gl::UNSIGNED_BYTE,
                        buffer.as_mut_ptr() as *mut std::ffi::c_void,
                    );
                }

                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

                // No buffer swap needed - we're not rendering to screen
            }

            println!("VideoPlayer: Render loop exiting");
        }
    }

    /// Render loop that runs in a separate thread (Windows)
    #[cfg(target_os = "windows")]
    fn render_loop(
        render_ctx: SendMpvRenderContext,
        gl_context: SendGLContext,
        shutdown: Arc<AtomicBool>,
        needs_render: Arc<AtomicBool>,
        fbo_id: u32,
        frame_buffer: Arc<Mutex<Vec<u8>>>,
        video_width: u32,
        video_height: u32,
    ) {
        unsafe {
            let mut frame_count = 0u64;
            println!("VideoPlayer: Windows render loop started");

            loop {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }

                // Wait for render flag or timeout
                if !needs_render.swap(false, Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(16)); // ~60 FPS
                    continue;
                }

                // Make WGL context current
                let hdc = HDC(gl_context.hdc as *mut _);
                let hglrc = HGLRC(gl_context.hglrc as *mut _);
                if let Err(e) = wglMakeCurrent(hdc, hglrc) {
                    eprintln!("Failed to make WGL context current in render loop: {:?}", e);
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    continue;
                }

                // Set up render parameters - render to our custom FBO
                let opengl_fbo = mpv_opengl_fbo {
                    fbo: fbo_id as i32,
                    w: video_width as i32,
                    h: video_height as i32,
                    internal_format: 0,
                };
                let flip_y: i32 = 0; // Don't flip for FBO

                let mut render_params: Vec<mpv_render_param> = vec![
                    mpv_render_param {
                        type_: mpv_render_param_type_MPV_RENDER_PARAM_OPENGL_FBO,
                        data: &opengl_fbo as *const _ as *mut c_void,
                    },
                    mpv_render_param {
                        type_: mpv_render_param_type_MPV_RENDER_PARAM_FLIP_Y,
                        data: &flip_y as *const _ as *mut c_void,
                    },
                    mpv_render_param {
                        type_: mpv_render_param_type_MPV_RENDER_PARAM_INVALID,
                        data: ptr::null_mut(),
                    },
                ];

                // Render to FBO
                mpv_render_context_render(render_ctx.0, render_params.as_mut_ptr());

                // Read pixels from FBO into frame buffer
                gl::BindFramebuffer(gl::FRAMEBUFFER, fbo_id);

                if let Ok(mut buffer) = frame_buffer.lock() {
                    // Use BGRA format to match video color ordering
                    gl::ReadPixels(
                        0,
                        0,
                        video_width as i32,
                        video_height as i32,
                        gl::BGRA,
                        gl::UNSIGNED_BYTE,
                        buffer.as_mut_ptr() as *mut std::ffi::c_void,
                    );
                }

                gl::BindFramebuffer(gl::FRAMEBUFFER, 0);

                // No buffer swap needed - we're not rendering to screen

                frame_count += 1;
                if frame_count % 60 == 0 {
                    println!("VideoPlayer: Rendered {} frames ({}x{})", frame_count, video_width, video_height);
                }
            }

            println!("VideoPlayer: Render loop exiting (rendered {} frames)", frame_count);
        }
    }

    /// Load a video file
    pub fn load_file(&mut self, file_path: &str) -> Result<(), VideoPlayerError> {
        if !std::path::Path::new(file_path).exists() {
            return Err(VideoPlayerError::InvalidFilePath);
        }

        println!("VideoPlayer: Loading file: {}", file_path);

        // Initialize mpv if not already done
        if self.event_thread.is_none() {
            self.initialize()?;

            // Now that mpv is initialized, create the render context if we have a GL context
            if self.gl_context.is_some() && self.render_context.is_none() {
                self.create_render_context();
            }
        }

        // Send loadfile command
        unsafe {
            let cmd = CString::new("loadfile").unwrap();
            let path = CString::new(file_path).unwrap();
            let mut args = [cmd.as_ptr(), path.as_ptr(), ptr::null()];

            let ret = mpv_command(self.mpv_handle.0, args.as_mut_ptr());
            if ret < 0 {
                return Err(VideoPlayerError::CommandError(Self::error_string(ret)));
            }
        }

        // Set to paused state initially (to match GStreamer behavior)
        self.pause()?;

        println!("VideoPlayer: File loaded successfully");
        Ok(())
    }

    /// Start message watch (no-op for mpv - event loop handles this)
    pub fn start_message_watch(&mut self) -> Result<(), VideoPlayerError> {
        Ok(())
    }

    /// Start playback
    pub fn play(&self) -> Result<(), VideoPlayerError> {
        println!("VideoPlayer: Starting playback");
        self.set_property_flag("pause", false)
    }

    /// Pause playback
    pub fn pause(&self) -> Result<(), VideoPlayerError> {
        println!("VideoPlayer: Pausing playback");
        self.set_property_flag("pause", true)
    }

    /// Stop playback
    pub fn stop(&self) -> Result<(), VideoPlayerError> {
        println!("VideoPlayer: Stopping playback");
        unsafe {
            let cmd = CString::new("stop").unwrap();
            let mut args = [cmd.as_ptr(), ptr::null()];
            let ret = mpv_command(self.mpv_handle.0, args.as_mut_ptr());
            if ret < 0 {
                return Err(VideoPlayerError::CommandError(Self::error_string(ret)));
            }
        }
        Ok(())
    }

    /// Get current playback position and duration
    pub fn get_position_duration(&self) -> Option<(ClockTime, ClockTime)> {
        let position = ClockTime(self.state.get_position());
        let duration = ClockTime(self.state.get_duration());
        Some((position, duration))
    }

    /// Check if video is playing
    pub fn is_playing(&self) -> bool {
        !self.state.is_paused()
    }

    /// Seek to a specific position
    pub fn seek(&self, position: ClockTime) -> Result<(), VideoPlayerError> {
        let pos_secs = position.seconds().unwrap_or(0.0);
        println!("VideoPlayer: Seeking to {:.2}s", pos_secs);

        unsafe {
            let cmd = CString::new("seek").unwrap();
            let pos = CString::new(format!("{}", pos_secs)).unwrap();
            let absolute = CString::new("absolute").unwrap();
            let mut args = [cmd.as_ptr(), pos.as_ptr(), absolute.as_ptr(), ptr::null()];

            let ret = mpv_command(self.mpv_handle.0, args.as_mut_ptr());
            if ret < 0 {
                return Err(VideoPlayerError::CommandError(Self::error_string(ret)));
            }
        }
        Ok(())
    }

    /// Enable or disable subtitle display
    pub fn set_subtitle_display(
        &self,
        enabled: bool,
        track_index: Option<i32>,
    ) -> Result<(), VideoPlayerError> {
        if enabled {
            if let Some(index) = track_index {
                println!("VideoPlayer: Enabling subtitle track {}", index);
                self.set_property_int("sid", index as i64)
            } else {
                Err(VideoPlayerError::CommandError(
                    "No track index provided".to_string(),
                ))
            }
        } else {
            println!("VideoPlayer: Disabling subtitles");
            // Set sid to "no" to disable
            unsafe {
                let name = CString::new("sid").unwrap();
                let value = CString::new("no").unwrap();
                let ret = mpv_set_property_string(self.mpv_handle.0, name.as_ptr(), value.as_ptr());
                if ret < 0 {
                    return Err(VideoPlayerError::MpvError(Self::error_string(ret)));
                }
            }
            Ok(())
        }
    }

    /// Set subtitle track
    pub fn set_subtitle_track(&self, track_index: i32) -> Result<(), VideoPlayerError> {
        println!("VideoPlayer: Setting subtitle track to {}", track_index);
        self.set_property_int("sid", track_index as i64)
    }

    /// Get pipeline reference (compatibility - returns None for mpv)
    pub fn get_pipeline(&self) -> Option<()> {
        None
    }

    /// Helper to set a boolean property
    fn set_property_flag(&self, name: &str, value: bool) -> Result<(), VideoPlayerError> {
        unsafe {
            let name_c = CString::new(name).unwrap();
            let mut flag: c_int = if value { 1 } else { 0 };
            let ret = mpv_set_property(
                self.mpv_handle.0,
                name_c.as_ptr(),
                mpv_format_MPV_FORMAT_FLAG,
                &mut flag as *mut c_int as *mut c_void,
            );
            if ret < 0 {
                return Err(VideoPlayerError::MpvError(Self::error_string(ret)));
            }
        }
        Ok(())
    }

    /// Helper to set an integer property
    fn set_property_int(&self, name: &str, value: i64) -> Result<(), VideoPlayerError> {
        unsafe {
            let name_c = CString::new(name).unwrap();
            let mut val = value;
            let ret = mpv_set_property(
                self.mpv_handle.0,
                name_c.as_ptr(),
                mpv_format_MPV_FORMAT_INT64,
                &mut val as *mut i64 as *mut c_void,
            );
            if ret < 0 {
                return Err(VideoPlayerError::MpvError(Self::error_string(ret)));
            }
        }
        Ok(())
    }

    /// Convert mpv error code to string
    fn error_string(error: c_int) -> String {
        unsafe {
            let ptr = mpv_error_string(error);
            if ptr.is_null() {
                return format!("Unknown error ({})", error);
            }
            CStr::from_ptr(ptr)
                .to_str()
                .unwrap_or("Invalid UTF-8")
                .to_string()
        }
    }

    /// Get a reference to the frame buffer for rendering in GPUI
    pub fn get_frame_buffer(&self) -> Arc<Mutex<Vec<u8>>> {
        Arc::clone(&self.frame_buffer)
    }

    /// Get video dimensions
    pub fn get_video_dimensions(&self) -> (u32, u32) {
        (self.video_width, self.video_height)
    }
}

impl Drop for VideoPlayer {
    fn drop(&mut self) {
        println!("VideoPlayer: Cleaning up");

        // Signal shutdown
        self.shutdown.store(true, Ordering::SeqCst);

        // Wait for render thread
        if let Some(thread) = self.render_thread.take() {
            let _ = thread.join();
        }

        // Wait for event thread
        if let Some(thread) = self.event_thread.take() {
            let _ = thread.join();
        }

        // Destroy render context
        unsafe {
            if let Some(render_ctx) = self.render_context.take() {
                if !render_ctx.0.is_null() {
                    mpv_render_context_free(render_ctx.0);
                }
            }
        }

        // Destroy mpv handle
        unsafe {
            if !self.mpv_handle.0.is_null() {
                mpv_terminate_destroy(self.mpv_handle.0);
                self.mpv_handle.0 = ptr::null_mut();
            }
        }

        // Release OpenGL context (platform-specific)
        #[cfg(target_os = "macos")]
        unsafe {
            if let Some(gl_ctx) = self.gl_context.take() {
                let () = msg_send![gl_ctx.0, release];
            }
        }

        #[cfg(target_os = "windows")]
        unsafe {
            if let Some(gl_ctx) = self.gl_context.take() {
                let hdc = HDC(gl_ctx.hdc as *mut _);
                let hglrc = HGLRC(gl_ctx.hglrc as *mut _);
                let hwnd = HWND(gl_ctx.hwnd as *mut _);

                wglMakeCurrent(HDC(std::ptr::null_mut()), HGLRC(std::ptr::null_mut()));
                wglDeleteContext(hglrc);
                ReleaseDC(Some(hwnd), hdc);
            }
        }
    }
}

/// Compatibility type for GStreamer ClockTime
#[derive(Debug, Clone, Copy)]
pub struct ClockTime(pub u64);

impl ClockTime {
    /// Create from nanoseconds
    pub fn from_nseconds(ns: u64) -> Self {
        ClockTime(ns)
    }

    /// Convert to seconds
    pub fn seconds(&self) -> Option<f64> {
        Some(self.0 as f64 / 1_000_000_000.0)
    }

    /// Get nanoseconds value
    pub fn nseconds(&self) -> u64 {
        self.0
    }
}

/// Initialize mpv libraries
pub fn init() -> Result<(), VideoPlayerError> {
    println!("mpv libraries initialized");

    unsafe {
        let version = mpv_client_api_version();
        println!("mpv client API version: {}", version);
    }

    Ok(())
}
