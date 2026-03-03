//! CGEventTap-based pass-through key listener for macOS.
//!
//! Uses `ListenOnly` mode so keypresses are observed but NOT consumed —
//! the game still receives the keystrokes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

// ── macOS implementation ────────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod platform {
    use core_foundation::base::TCFType;
    use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop, CFRunLoopRef};
    use core_graphics::event::{
        CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
        CGEventType,
    };
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    };

    /// Virtual keycode → key string mapping (matches frontend `buildHotkeyString`).
    fn keycode_to_string(keycode: u16) -> Option<String> {
        let s = match keycode {
            0 => "A",
            1 => "S",
            2 => "D",
            3 => "F",
            4 => "H",
            5 => "G",
            6 => "Z",
            7 => "X",
            8 => "C",
            9 => "V",
            11 => "B",
            12 => "Q",
            13 => "W",
            14 => "E",
            15 => "R",
            16 => "Y",
            17 => "T",
            18 => "1",
            19 => "2",
            20 => "3",
            21 => "4",
            22 => "6",
            23 => "5",
            24 => "=",
            25 => "9",
            26 => "7",
            27 => "-",
            28 => "8",
            29 => "0",
            30 => "]",
            31 => "O",
            32 => "U",
            33 => "[",
            34 => "I",
            35 => "P",
            37 => "L",
            38 => "J",
            39 => "'",
            40 => "K",
            41 => ";",
            42 => "\\",
            43 => ",",
            44 => "/",
            45 => "N",
            46 => "M",
            47 => ".",
            49 => "Space",
            50 => "Backquote",
            51 => "Backspace",
            53 => "Escape",
            65 => ".",      // numpad decimal
            67 => "*",      // numpad multiply
            69 => "+",      // numpad plus
            71 => "Clear",  // numpad clear
            75 => "/",      // numpad divide
            76 => "Enter",  // numpad enter
            78 => "-",      // numpad minus
            82 => "0",      // numpad 0
            83 => "1",      // numpad 1
            84 => "2",      // numpad 2
            85 => "3",      // numpad 3
            86 => "4",      // numpad 4
            87 => "5",      // numpad 5
            88 => "6",      // numpad 6
            89 => "7",      // numpad 7
            91 => "8",      // numpad 8
            92 => "9",      // numpad 9
            36 => "Enter",
            48 => "Tab",
            96 => "F5",
            97 => "F6",
            98 => "F7",
            99 => "F3",
            100 => "F8",
            101 => "F9",
            103 => "F11",
            105 => "F13",
            109 => "F10",
            111 => "F12",
            113 => "F14",
            115 => "Home",
            116 => "PageUp",
            117 => "Delete",
            118 => "F4",
            119 => "End",
            120 => "F2",
            121 => "PageDown",
            122 => "F1",
            123 => "Left",
            124 => "Right",
            125 => "Down",
            126 => "Up",
            _ => return None,
        };
        Some(s.to_string())
    }

    /// Build a hotkey string from CGEvent flags + keycode, matching the
    /// frontend `buildHotkeyString` format (e.g. "Ctrl+Shift+A", "O").
    fn build_hotkey_string(keycode: u16, flags: core_graphics::event::CGEventFlags) -> Option<String> {
        let key_str = keycode_to_string(keycode)?;

        let mut parts: Vec<&str> = Vec::new();

        // Check modifier flags
        let ctrl = flags.contains(core_graphics::event::CGEventFlags::CGEventFlagControl)
            || flags.contains(core_graphics::event::CGEventFlags::CGEventFlagCommand);
        let shift = flags.contains(core_graphics::event::CGEventFlags::CGEventFlagShift);
        let alt = flags.contains(core_graphics::event::CGEventFlags::CGEventFlagAlternate);

        if ctrl {
            parts.push("Ctrl");
        }
        if shift {
            parts.push("Shift");
        }
        if alt {
            parts.push("Alt");
        }
        parts.push(&key_str);

        Some(parts.join("+"))
    }

    pub type Callback = Arc<dyn Fn(String) + Send + Sync>;

    /// Wrapper to make CFRunLoopRef Send+Sync.
    /// SAFETY: CFRunLoopStop is documented as thread-safe by Apple.
    struct SendableRunLoopRef(CFRunLoopRef);
    unsafe impl Send for SendableRunLoopRef {}
    unsafe impl Sync for SendableRunLoopRef {}

    pub struct PlatformListener {
        running: Arc<AtomicBool>,
        runloop_ref: Arc<Mutex<Option<SendableRunLoopRef>>>,
        hotkeys: Arc<Mutex<HashMap<String, String>>>, // hotkey_string → buff_id
        thread_handle: Mutex<Option<std::thread::JoinHandle<()>>>,
    }

    impl PlatformListener {
        pub fn new() -> Self {
            Self {
                running: Arc::new(AtomicBool::new(false)),
                runloop_ref: Arc::new(Mutex::new(None)),
                hotkeys: Arc::new(Mutex::new(HashMap::new())),
                thread_handle: Mutex::new(None),
            }
        }

        pub fn start(&self, callback: Callback) -> Result<(), String> {
            if self.running.load(Ordering::SeqCst) {
                return Ok(()); // already running
            }

            let running = self.running.clone();
            let runloop_ref = self.runloop_ref.clone();
            let hotkeys = self.hotkeys.clone();

            let handle = std::thread::spawn(move || {
                // Create the event tap (listen-only, key down events)
                let tap = CGEventTap::new(
                    CGEventTapLocation::Session,
                    CGEventTapPlacement::HeadInsertEventTap,
                    CGEventTapOptions::ListenOnly,
                    vec![CGEventType::KeyDown],
                    move |_proxy, _event_type, event: &CGEvent| -> Option<CGEvent> {
                        let keycode = event.get_integer_value_field(
                            core_graphics::event::EventField::KEYBOARD_EVENT_KEYCODE,
                        ) as u16;
                        let flags = event.get_flags();

                        if let Some(hotkey_str) = build_hotkey_string(keycode, flags) {
                            let map = hotkeys.lock().unwrap();
                            if let Some(buff_id) = map.get(&hotkey_str) {
                                let bid = buff_id.clone();
                                let cb = callback.clone();
                                // Fire callback (non-blocking)
                                std::thread::spawn(move || {
                                    cb(bid);
                                });
                            }
                        }
                        // Return None in ListenOnly mode — event passes through regardless
                        None
                    },
                );

                let tap = match tap {
                    Ok(tap) => tap,
                    Err(_) => {
                        eprintln!("[key_listener] Failed to create CGEventTap — accessibility permission required");
                        return;
                    }
                };

                // Add tap to current thread's run loop
                let source = tap
                    .mach_port
                    .create_runloop_source(0)
                    .expect("Failed to create runloop source");

                let current_loop = CFRunLoop::get_current();
                unsafe {
                    current_loop.add_source(&source, kCFRunLoopCommonModes);
                }

                tap.enable();

                // Store run loop ref so stop() can wake it
                {
                    let mut rl = runloop_ref.lock().unwrap();
                    *rl = Some(SendableRunLoopRef(current_loop.as_concrete_TypeRef()));
                }

                running.store(true, Ordering::SeqCst);

                // Run the loop (blocks until stopped)
                CFRunLoop::run_current();

                running.store(false, Ordering::SeqCst);

                // Clear run loop ref
                {
                    let mut rl = runloop_ref.lock().unwrap();
                    *rl = None;
                }
            });

            // Wait briefly for the thread to start
            std::thread::sleep(std::time::Duration::from_millis(100));

            let mut th = self.thread_handle.lock().unwrap();
            *th = Some(handle);

            Ok(())
        }

        pub fn stop(&self) {
            if !self.running.load(Ordering::SeqCst) {
                return;
            }

            // Stop the CFRunLoop
            let rl = self.runloop_ref.lock().unwrap();
            if let Some(ref rl_ref) = *rl {
                unsafe {
                    core_foundation::runloop::CFRunLoopStop(rl_ref.0);
                }
            }
            drop(rl);

            // Wait for thread to finish
            let mut th = self.thread_handle.lock().unwrap();
            if let Some(handle) = th.take() {
                let _ = handle.join();
            }
        }

        pub fn update_hotkeys(&self, map: HashMap<String, String>) {
            let mut hotkeys = self.hotkeys.lock().unwrap();
            *hotkeys = map;
        }

        pub fn is_running(&self) -> bool {
            self.running.load(Ordering::SeqCst)
        }
    }

    impl Drop for PlatformListener {
        fn drop(&mut self) {
            self.stop();
        }
    }

    /// Check if accessibility permission is granted (AXIsProcessTrusted).
    pub fn check_accessibility() -> bool {
        // Link against ApplicationServices framework
        extern "C" {
            fn AXIsProcessTrusted() -> bool;
        }
        unsafe { AXIsProcessTrusted() }
    }

    /// Open System Preferences → Privacy → Accessibility pane.
    pub fn request_accessibility() {
        use std::process::Command;
        let _ = Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

// ── Non-macOS stub ──────────────────────────────────────────────────────────

#[cfg(not(target_os = "macos"))]
mod platform {
    use std::collections::HashMap;
    use std::sync::Arc;

    pub type Callback = Arc<dyn Fn(String) + Send + Sync>;

    pub struct PlatformListener;

    impl PlatformListener {
        pub fn new() -> Self {
            Self
        }

        pub fn start(&self, _callback: Callback) -> Result<(), String> {
            Err("Key listener not supported on this platform".to_string())
        }

        pub fn stop(&self) {}

        pub fn update_hotkeys(&self, _map: HashMap<String, String>) {}

        pub fn is_running(&self) -> bool {
            false
        }
    }

    pub fn check_accessibility() -> bool {
        false
    }

    pub fn request_accessibility() {}
}

// ── Public API ──────────────────────────────────────────────────────────────

pub use platform::Callback;

pub struct KeyListener {
    inner: platform::PlatformListener,
    /// Tracks whether monitoring was requested (survives brief restarts).
    monitoring_requested: AtomicBool,
}

impl KeyListener {
    pub fn new() -> Self {
        Self {
            inner: platform::PlatformListener::new(),
            monitoring_requested: AtomicBool::new(false),
        }
    }

    /// Start listening. The callback receives a buff_id when a matching key is pressed.
    pub fn start(&self, callback: Callback) -> Result<(), String> {
        self.monitoring_requested.store(true, Ordering::SeqCst);
        self.inner.start(callback)
    }

    /// Stop listening.
    pub fn stop(&self) {
        self.monitoring_requested.store(false, Ordering::SeqCst);
        self.inner.stop();
    }

    /// Replace the hotkey→buff_id mapping.
    pub fn update_hotkeys(&self, map: HashMap<String, String>) {
        self.inner.update_hotkeys(map);
    }

    /// Whether the CGEventTap thread is currently running.
    pub fn is_running(&self) -> bool {
        self.inner.is_running()
    }

    /// Check macOS accessibility permission.
    pub fn check_accessibility() -> bool {
        platform::check_accessibility()
    }

    /// Prompt user to grant accessibility permission.
    pub fn request_accessibility() {
        platform::request_accessibility();
    }
}
