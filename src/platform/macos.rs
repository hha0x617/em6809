#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
use objc::runtime::{Class, Object, Sel};
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};
#[cfg(target_os = "macos")]
use once_cell::sync::OnceCell;
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
unsafe fn nsstring(s: &str) -> *mut Object {
    let c = std::ffi::CString::new(s).unwrap_or_else(|_| std::ffi::CString::new("").unwrap());
    let ns: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
    ns
}

/// macOS: ensure the application (leftmost) menu shows a custom title, e.g., "em6809".
#[cfg(target_os = "macos")]
pub fn set_app_menu_title(name: &str) {
    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let main_menu: *mut Object = msg_send![app, mainMenu];
        if main_menu.is_null() {
            return;
        }
        let item0: *mut Object = msg_send![main_menu, itemAtIndex: 0i64];
        if item0.is_null() {
            return;
        }
        let ns_name: *mut Object = nsstring(name);
        let _: () = msg_send![item0, setTitle: ns_name];
    }
}

/// macOS: replace the default About handler to show a custom modal with version/license text.
#[cfg(target_os = "macos")]
pub fn install_about_handler_once() {
    use objc::declare::ClassDecl;
    use objc::runtime::Object as ObjcObject;
    use std::sync::atomic::{AtomicBool, Ordering};

    static INSTALLED: AtomicBool = AtomicBool::new(false);
    static mut HELPER_INSTANCE: *mut ObjcObject = std::ptr::null_mut();

    extern "C" fn show_about(_this: &ObjcObject, _sel: Sel, _sender: *mut ObjcObject) {
        // Show a simple modal About dialog via NSAlert with custom credits
        unsafe {
            let pool: *mut ObjcObject = msg_send![class!(NSAutoreleasePool), new];
            let alert: *mut ObjcObject = msg_send![class!(NSAlert), alloc];
            let alert: *mut ObjcObject = msg_send![alert, init];
            let _: () = msg_send![alert, setMessageText: nsstring("em6809")];
            let version = env!("CARGO_PKG_VERSION");
            let info = format!(
                "Version: {version}\n\nCopyright (c) 2025 hha0x617\n\nDual-licensed: MIT OR Apache-2.0\n\nSee LICENSE-MIT, LICENSE-APACHE, and NOTICE (if present)\n\nThis software is provided \"AS IS\" without warranty."
            );
            let _: () = msg_send![alert, setInformativeText: nsstring(&info)];
            let _: () = msg_send![alert, addButtonWithTitle: nsstring("OK")];
            // Set app icon if available
            let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
            let icon: *mut ObjcObject = msg_send![app, applicationIconImage];
            if !icon.is_null() {
                let _: () = msg_send![alert, setIcon: icon];
            }
            let _: i64 = msg_send![alert, runModal];
            let _: () = msg_send![alert, release];
            let _: () = msg_send![pool, drain];
        }
    }

    unsafe fn ensure_helper_class() -> &'static Class {
        let name = "Em6809AboutHelper";
        if let Some(cls) = Class::get(name) {
            return cls;
        }
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new(name, superclass).unwrap();
        decl.add_method(
            sel!(showAbout:),
            show_about as extern "C" fn(&ObjcObject, Sel, *mut ObjcObject),
        );
        decl.register()
    }

    unsafe fn ensure_helper_instance() -> *mut ObjcObject {
        if HELPER_INSTANCE.is_null() {
            let cls = ensure_helper_class();
            let obj: *mut ObjcObject = msg_send![cls, alloc];
            let obj: *mut ObjcObject = msg_send![obj, init];
            HELPER_INSTANCE = obj;
        }
        HELPER_INSTANCE
    }

    if INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        let main_menu: *mut ObjcObject = msg_send![app, mainMenu];
        if main_menu.is_null() {
            return;
        }
        let apple_item: *mut ObjcObject = msg_send![main_menu, itemAtIndex: 0i64];
        if apple_item.is_null() {
            return;
        }
        let submenu: *mut ObjcObject = msg_send![apple_item, submenu];
        if submenu.is_null() {
            return;
        }
        let count: i64 = msg_send![submenu, numberOfItems];
        let target = ensure_helper_instance();
        for i in 0..count {
            let item: *mut ObjcObject = msg_send![submenu, itemAtIndex: i];
            if item.is_null() {
                continue;
            }
            let action: Sel = msg_send![item, action];
            if action == sel!(orderFrontStandardAboutPanel:) {
                let _: () = msg_send![item, setTarget: target];
                let _: () = msg_send![item, setAction: sel!(showAbout:)];
                break;
            }
        }
    }
}

/// macOS: install a native main menu with a File menu in the system menubar.
/// Adds File > Open… with Cmd+O that triggers a registered Rust callback.
#[cfg(target_os = "macos")]
pub fn install_global_menubar_once(open_dialog_cb: Box<dyn Fn() + Send + Sync>) {
    use objc::declare::ClassDecl;
    use objc::runtime::Object as ObjcObject;
    static MENU_INSTALLED: AtomicBool = AtomicBool::new(false);
    static mut MENU_HELPER_INSTANCE: *mut ObjcObject = std::ptr::null_mut();
    static OPEN_CB: OnceCell<Box<dyn Fn() + Send + Sync>> = OnceCell::new();

    extern "C" fn em_open_file(_this: &ObjcObject, _sel: Sel, _sender: *mut ObjcObject) {
        if let Some(cb) = OPEN_CB.get() {
            (cb)();
        }
    }

    unsafe fn ensure_menu_helper_class() -> &'static Class {
        let name = "Em6809MenuHelper";
        if let Some(cls) = Class::get(name) {
            return cls;
        }
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new(name, superclass).unwrap();
        decl.add_method(
            sel!(em_openFile:),
            em_open_file as extern "C" fn(&ObjcObject, Sel, *mut ObjcObject),
        );
        decl.register()
    }

    unsafe fn ensure_menu_helper_instance() -> *mut ObjcObject {
        if MENU_HELPER_INSTANCE.is_null() {
            let cls = ensure_menu_helper_class();
            let obj: *mut ObjcObject = msg_send![cls, alloc];
            let obj: *mut ObjcObject = msg_send![obj, init];
            MENU_HELPER_INSTANCE = obj;
        }
        MENU_HELPER_INSTANCE
    }

    if MENU_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = OPEN_CB.set(open_dialog_cb);
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let main_menu: *mut ObjcObject = msg_send![app, mainMenu];
        if main_menu.is_null() {
            // Create an empty main menu if none exists
            let new_menu: *mut ObjcObject = msg_send![class!(NSMenu), alloc];
            let new_menu: *mut ObjcObject = msg_send![new_menu, init];
            let _: () = msg_send![app, setMainMenu: new_menu];
        }
        let main_menu: *mut ObjcObject = msg_send![app, mainMenu];
        if main_menu.is_null() {
            return;
        }
        // Avoid duplicate File menu by scanning titles
        let count: i64 = msg_send![main_menu, numberOfItems];
        let mut has_file = false;
        for i in 0..count {
            let item: *mut ObjcObject = msg_send![main_menu, itemAtIndex: i];
            if item.is_null() {
                continue;
            }
            let title_ns: *mut ObjcObject = msg_send![item, title];
            if !title_ns.is_null() {
                let utf8: *const i8 = msg_send![title_ns, UTF8String];
                if !utf8.is_null() {
                    let s = std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string();
                    if s == "File" {
                        has_file = true;
                        break;
                    }
                }
            }
        }
        if !has_file {
            let file_menu_title: *mut ObjcObject = nsstring("File");
            let file_menu: *mut ObjcObject = msg_send![class!(NSMenu), alloc];
            let file_menu: *mut ObjcObject = msg_send![file_menu, initWithTitle: file_menu_title];

            // Open… item
            let open_title: *mut ObjcObject = nsstring("Open…");
            let key: *mut ObjcObject = nsstring("o");
            let open_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
            let open_item: *mut ObjcObject = msg_send![open_item, initWithTitle: open_title action: sel!(em_openFile:) keyEquivalent: key];
            // Cmd modifier (NSEventModifierFlagCommand = 1 << 20)
            let _: () = msg_send![open_item, setKeyEquivalentModifierMask: (1u64 << 20) as u64];
            // Target to helper
            let target = ensure_menu_helper_instance();
            let _: () = msg_send![open_item, setTarget: target];
            let _: () = msg_send![file_menu, addItem: open_item];

            // Create top-level menu item and attach submenu
            let top_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
            let top_item: *mut ObjcObject = msg_send![top_item, initWithTitle: file_menu_title action: core::ptr::null::<()>() keyEquivalent: nsstring("")];
            let _: () = msg_send![top_item, setSubmenu: file_menu];
            let _: () = msg_send![main_menu, addItem: top_item];
        }
    }
}

/// macOS: install a "View" submenu next to File, with toggles for the
/// Breakpoints and Call Stack child windows. Each callback flips the
/// corresponding `*_window_open` flag in `RegApp`. Checkmark state is
/// kept in sync with the actual flag via [`set_view_menu_check_states`].
///
/// Mirrors what the egui in-app `View` menu does on Windows / Linux.
/// Without this menu the only way to toggle these windows on macOS
/// would be the OS-level X button (open has no equivalent affordance).
#[cfg(target_os = "macos")]
pub fn install_view_menu_once(
    toggle_bp_cb: Box<dyn Fn() + Send + Sync>,
    toggle_cs_cb: Box<dyn Fn() + Send + Sync>,
) {
    use objc::declare::ClassDecl;
    use objc::runtime::Object as ObjcObject;
    static VIEW_INSTALLED: AtomicBool = AtomicBool::new(false);
    static mut VIEW_HELPER_INSTANCE: *mut ObjcObject = std::ptr::null_mut();
    static TOGGLE_BP_CB: OnceCell<Box<dyn Fn() + Send + Sync>> = OnceCell::new();
    static TOGGLE_CS_CB: OnceCell<Box<dyn Fn() + Send + Sync>> = OnceCell::new();

    extern "C" fn em_toggle_bp(_this: &ObjcObject, _sel: Sel, _sender: *mut ObjcObject) {
        if let Some(cb) = TOGGLE_BP_CB.get() {
            (cb)();
        }
    }
    extern "C" fn em_toggle_cs(_this: &ObjcObject, _sel: Sel, _sender: *mut ObjcObject) {
        if let Some(cb) = TOGGLE_CS_CB.get() {
            (cb)();
        }
    }

    unsafe fn ensure_view_helper_class() -> &'static Class {
        let name = "Em6809ViewMenuHelper";
        if let Some(cls) = Class::get(name) {
            return cls;
        }
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new(name, superclass).unwrap();
        decl.add_method(
            sel!(em_toggleBp:),
            em_toggle_bp as extern "C" fn(&ObjcObject, Sel, *mut ObjcObject),
        );
        decl.add_method(
            sel!(em_toggleCs:),
            em_toggle_cs as extern "C" fn(&ObjcObject, Sel, *mut ObjcObject),
        );
        decl.register()
    }

    unsafe fn ensure_view_helper_instance() -> *mut ObjcObject {
        if VIEW_HELPER_INSTANCE.is_null() {
            let cls = ensure_view_helper_class();
            let obj: *mut ObjcObject = msg_send![cls, alloc];
            let obj: *mut ObjcObject = msg_send![obj, init];
            VIEW_HELPER_INSTANCE = obj;
        }
        VIEW_HELPER_INSTANCE
    }

    if VIEW_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = TOGGLE_BP_CB.set(toggle_bp_cb);
    let _ = TOGGLE_CS_CB.set(toggle_cs_cb);
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let main_menu: *mut ObjcObject = msg_send![app, mainMenu];
        if main_menu.is_null() {
            return;
        }
        // Skip if a View menu already exists (rebuild safety).
        let count: i64 = msg_send![main_menu, numberOfItems];
        for i in 0..count {
            let item: *mut ObjcObject = msg_send![main_menu, itemAtIndex: i];
            if item.is_null() {
                continue;
            }
            let title_ns: *mut ObjcObject = msg_send![item, title];
            if !title_ns.is_null() {
                let utf8: *const i8 = msg_send![title_ns, UTF8String];
                if !utf8.is_null() {
                    let s = std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string();
                    if s == "View" {
                        return;
                    }
                }
            }
        }

        let view_title: *mut ObjcObject = nsstring("View");
        let view_menu: *mut ObjcObject = msg_send![class!(NSMenu), alloc];
        let view_menu: *mut ObjcObject = msg_send![view_menu, initWithTitle: view_title];

        let target = ensure_view_helper_instance();

        let bp_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let bp_item: *mut ObjcObject = msg_send![bp_item, initWithTitle: nsstring("Breakpoints") action: sel!(em_toggleBp:) keyEquivalent: nsstring("")];
        let _: () = msg_send![bp_item, setTarget: target];
        let _: () = msg_send![view_menu, addItem: bp_item];

        let cs_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let cs_item: *mut ObjcObject = msg_send![cs_item, initWithTitle: nsstring("Call Stack") action: sel!(em_toggleCs:) keyEquivalent: nsstring("")];
        let _: () = msg_send![cs_item, setTarget: target];
        let _: () = msg_send![view_menu, addItem: cs_item];

        let top_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let top_item: *mut ObjcObject = msg_send![top_item, initWithTitle: view_title action: core::ptr::null::<()>() keyEquivalent: nsstring("")];
        let _: () = msg_send![top_item, setSubmenu: view_menu];
        let _: () = msg_send![main_menu, addItem: top_item];

        VIEW_BP_ITEM.store(bp_item as usize, Ordering::SeqCst);
        VIEW_CS_ITEM.store(cs_item as usize, Ordering::SeqCst);
    }
}

#[cfg(target_os = "macos")]
static VIEW_BP_ITEM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(target_os = "macos")]
static VIEW_CS_ITEM: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// macOS: mirror the current open-state of the Breakpoints / Call Stack
/// child windows onto the View menu items as NSMenuItem checkmarks.
/// Idempotent — safe to call every frame.
#[cfg(target_os = "macos")]
pub fn set_view_menu_check_states(bp_on: bool, cs_on: bool) {
    use objc::runtime::Object as ObjcObject;
    // NSControlStateValue: Off=0, On=1.
    let bp_state: i64 = if bp_on { 1 } else { 0 };
    let cs_state: i64 = if cs_on { 1 } else { 0 };
    unsafe {
        let bp = VIEW_BP_ITEM.load(Ordering::SeqCst) as *mut ObjcObject;
        if !bp.is_null() {
            let _: () = msg_send![bp, setState: bp_state];
        }
        let cs = VIEW_CS_ITEM.load(Ordering::SeqCst) as *mut ObjcObject;
        if !cs.is_null() {
            let _: () = msg_send![cs, setState: cs_state];
        }
    }
}

/// macOS: install a handler and menu item for Preferences… under the App menu, calling back into Rust to open Settings.
#[cfg(target_os = "macos")]
pub fn install_preferences_handler_once(open_prefs_cb: Box<dyn Fn() + Send + Sync>) {
    use objc::declare::ClassDecl;
    use objc::runtime::Object as ObjcObject;
    static PREFS_INSTALLED: AtomicBool = AtomicBool::new(false);
    static mut PREFS_HELPER_INSTANCE: *mut ObjcObject = std::ptr::null_mut();
    static PREFS_CB: OnceCell<Box<dyn Fn() + Send + Sync>> = OnceCell::new();

    extern "C" fn em_open_prefs(_this: &ObjcObject, _sel: Sel, _sender: *mut ObjcObject) {
        if let Some(cb) = PREFS_CB.get() {
            (cb)();
        }
    }

    unsafe fn ensure_prefs_helper_class() -> &'static Class {
        let name = "Em6809PrefsHelper";
        if let Some(cls) = Class::get(name) {
            return cls;
        }
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new(name, superclass).unwrap();
        decl.add_method(
            sel!(em_openPrefs:),
            em_open_prefs as extern "C" fn(&ObjcObject, Sel, *mut ObjcObject),
        );
        decl.register()
    }

    unsafe fn ensure_prefs_helper_instance() -> *mut ObjcObject {
        if PREFS_HELPER_INSTANCE.is_null() {
            let cls = ensure_prefs_helper_class();
            let obj: *mut ObjcObject = msg_send![cls, alloc];
            let obj: *mut ObjcObject = msg_send![obj, init];
            PREFS_HELPER_INSTANCE = obj;
        }
        PREFS_HELPER_INSTANCE
    }

    if PREFS_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = PREFS_CB.set(open_prefs_cb);
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        let main_menu: *mut ObjcObject = msg_send![app, mainMenu];
        if main_menu.is_null() {
            return;
        }
        // App menu is item 0
        let apple_item: *mut ObjcObject = msg_send![main_menu, itemAtIndex: 0i64];
        if apple_item.is_null() {
            return;
        }
        let submenu: *mut ObjcObject = msg_send![apple_item, submenu];
        if submenu.is_null() {
            return;
        }
        // Insert Preferences… after About (typically index 1)
        let prefs_title: *mut ObjcObject = nsstring("Preferences…");
        let key: *mut ObjcObject = nsstring(","); // Cmd+,
        let item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let item: *mut ObjcObject = msg_send![item, initWithTitle: prefs_title action: sel!(em_openPrefs:) keyEquivalent: key];
        let target = ensure_prefs_helper_instance();
        let _: () = msg_send![item, setTarget: target];
        // Add with a reasonable index (after About)
        let _: () = msg_send![submenu, insertItem: item atIndex: 1i64];
    }
}

#[cfg(target_os = "macos")]
pub fn install_native_settings_handler_once(open_native_cb: Box<dyn Fn() + Send + Sync>) {
    use objc::declare::ClassDecl;
    use objc::runtime::Object as ObjcObject;
    static NATIVE_INSTALLED: AtomicBool = AtomicBool::new(false);
    static mut NATIVE_HELPER_INSTANCE: *mut ObjcObject = std::ptr::null_mut();
    static NATIVE_CB: OnceCell<Box<dyn Fn() + Send + Sync>> = OnceCell::new();

    extern "C" fn em_open_native(_this: &ObjcObject, _sel: Sel, _sender: *mut ObjcObject) {
        if let Some(cb) = NATIVE_CB.get() {
            (cb)();
        }
    }

    unsafe fn ensure_native_helper_class() -> &'static Class {
        let name = "Em6809NativeSettingsHelper";
        if let Some(cls) = Class::get(name) {
            return cls;
        }
        let superclass = Class::get("NSObject").unwrap();
        let mut decl = ClassDecl::new(name, superclass).unwrap();
        decl.add_method(
            sel!(em_openNative:),
            em_open_native as extern "C" fn(&ObjcObject, Sel, *mut ObjcObject),
        );
        decl.register()
    }

    unsafe fn ensure_native_helper_instance() -> *mut ObjcObject {
        if NATIVE_HELPER_INSTANCE.is_null() {
            let cls = ensure_native_helper_class();
            let obj: *mut ObjcObject = msg_send![cls, alloc];
            let obj: *mut ObjcObject = msg_send![obj, init];
            NATIVE_HELPER_INSTANCE = obj;
        }
        NATIVE_HELPER_INSTANCE
    }

    if NATIVE_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = NATIVE_CB.set(open_native_cb);
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        let main_menu: *mut ObjcObject = msg_send![app, mainMenu];
        if main_menu.is_null() {
            return;
        }
        let apple_item: *mut ObjcObject = msg_send![main_menu, itemAtIndex: 0i64];
        if apple_item.is_null() {
            return;
        }
        let submenu: *mut ObjcObject = msg_send![apple_item, submenu];
        if submenu.is_null() {
            return;
        }
        let title: *mut ObjcObject = nsstring("Settings (Native)…");
        let key: *mut ObjcObject = nsstring("");
        let item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let item: *mut ObjcObject =
            msg_send![item, initWithTitle: title action: sel!(em_openNative:) keyEquivalent: key];
        let target = ensure_native_helper_instance();
        let _: () = msg_send![item, setTarget: target];
        let _: () = msg_send![submenu, addItem: item];
    }
}

// ---------------------------------------------------------------
// macOS focus management for eframe 0.29 multi-viewport
// ---------------------------------------------------------------
//
// Two related symptoms with auxiliary NSWindows:
//
//   1. Read-only panels (Breakpoints / Call Stack) keep promoting
//      themselves to key window during paint cycles, blocking the main
//      window's TextEdit / register-edit controls from receiving keys.
//
//   2. Console needs to receive keystrokes (the user types into the
//      running em6809 program), but its paint cycle also pulls key
//      window status away from main during normal emulator output.
//
// Approach: install class-level method swizzles on the winit NSWindow
// class and consult a per-instance marker in the swizzled methods.
//
//   marker == NON_ACTIVATING   → `canBecomeKeyWindow` returns NO,
//                                 `canBecomeMainWindow` returns NO.
//   marker == CLICK_ACTIVATED  → flag-driven; flips on mouse-down via
//                                 `sendEvent:`, drops on `resignKeyWindow`.
//   marker absent (main, etc.) → fall through to the original IMP.
//
// Why method swizzling and not `object_setClass:` to a custom subclass?
// winit registers a `effectiveAppearance` KVO observer on each NSWindow
// for theme tracking.  KVO uses isa-swizzling internally to install its
// notify-on-set subclass; if we swap isa AFTER KVO has set it up, the
// observer registration becomes invisible and `removeObserver:forKeyPath:`
// raises NSRangeException at app shutdown.  Modifying the class itself
// (rather than the per-instance isa) leaves KVO bookkeeping untouched.
//
// All swizzled IMPs delegate to the original (saved BEFORE replacement)
// for unmarked instances, so the main window's behaviour is identical
// to a clean winit run.

/// Per-window focus behaviour.  `ClickActivated` carries a leaked
/// `&'static AtomicBool` so each click-activated window has an
/// independent flag — sharing one global flag across multiple windows
/// (e.g., Console + Breakpoints) would let them flip each other's
/// key-window eligibility, breaking the per-window UX.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
enum FocusMarker {
    NonActivating,
    ClickActivated(&'static AtomicBool),
}

// Type encoding strings for the four selectors we override.
// `BOOL` is `_Bool` (encoding "B") on aarch64 and `signed char` ("c")
// on x86_64 macOS, so we pick at compile time.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const FOCUS_ENC_BOOL_NO_ARGS: &[u8] = b"B@:\0";
#[cfg(all(target_os = "macos", not(target_arch = "aarch64")))]
const FOCUS_ENC_BOOL_NO_ARGS: &[u8] = b"c@:\0";
#[cfg(target_os = "macos")]
const FOCUS_ENC_VOID_ID: &[u8] = b"v@:@\0";
#[cfg(target_os = "macos")]
const FOCUS_ENC_VOID_NO_ARGS: &[u8] = b"v@:\0";

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct FocusOrigImps {
    can_become_key: usize,
    can_become_main: usize,
    send_event: usize,
    resign_key: usize,
}

#[cfg(target_os = "macos")]
static FOCUS_MARKERS: OnceCell<std::sync::Mutex<std::collections::HashMap<usize, FocusMarker>>> =
    OnceCell::new();
#[cfg(target_os = "macos")]
static FOCUS_SWIZZLE_ORIGS: OnceCell<
    std::sync::Mutex<std::collections::HashMap<usize, FocusOrigImps>>,
> = OnceCell::new();

#[cfg(target_os = "macos")]
fn focus_marker_of(ptr: usize) -> Option<FocusMarker> {
    FOCUS_MARKERS
        .get()
        .and_then(|m| m.lock().ok().and_then(|g| g.get(&ptr).copied()))
}

/// Walk up the class hierarchy from `cls` to find the first class for
/// which we have stored swizzle originals.  Needed because KVO can
/// produce dynamic subclasses (NSKVONotifying_*) whose actual class
/// pointer differs from the one we swizzled — but the class chain
/// still includes the winit NSWindow class we modified.
#[cfg(target_os = "macos")]
unsafe fn focus_origs_for(mut cls: *const Class) -> Option<FocusOrigImps> {
    extern "C" {
        fn class_getSuperclass(cls: *const Class) -> *const Class;
    }
    let map = FOCUS_SWIZZLE_ORIGS.get()?;
    let g = map.lock().ok()?;
    while !cls.is_null() {
        if let Some(imps) = g.get(&(cls as usize)) {
            return Some(*imps);
        }
        cls = class_getSuperclass(cls);
    }
    None
}

#[cfg(target_os = "macos")]
extern "C" fn focus_can_become_key_swizzled(this: &Object, sel: Sel) -> objc::runtime::BOOL {
    use objc::runtime::{BOOL, NO, YES};
    let ptr = this as *const Object as usize;
    match focus_marker_of(ptr) {
        Some(FocusMarker::NonActivating) => return NO,
        Some(FocusMarker::ClickActivated(flag)) => {
            return if flag.load(Ordering::SeqCst) { YES } else { NO };
        }
        None => {}
    }
    unsafe {
        let cls: *const Class = msg_send![this as *const Object as *mut Object, class];
        if let Some(imps) = focus_origs_for(cls) {
            type Sig = extern "C" fn(&Object, Sel) -> BOOL;
            let f: Sig = std::mem::transmute::<*const std::os::raw::c_void, Sig>(
                imps.can_become_key as *const std::os::raw::c_void,
            );
            return f(this, sel);
        }
        YES
    }
}

#[cfg(target_os = "macos")]
extern "C" fn focus_can_become_main_swizzled(this: &Object, sel: Sel) -> objc::runtime::BOOL {
    use objc::runtime::{BOOL, NO};
    let ptr = this as *const Object as usize;
    if focus_marker_of(ptr).is_some() {
        return NO;
    }
    unsafe {
        let cls: *const Class = msg_send![this as *const Object as *mut Object, class];
        if let Some(imps) = focus_origs_for(cls) {
            type Sig = extern "C" fn(&Object, Sel) -> BOOL;
            let f: Sig = std::mem::transmute::<*const std::os::raw::c_void, Sig>(
                imps.can_become_main as *const std::os::raw::c_void,
            );
            return f(this, sel);
        }
        objc::runtime::YES
    }
}

#[cfg(target_os = "macos")]
extern "C" fn focus_send_event_swizzled(this: &Object, sel: Sel, event: *mut Object) {
    let ptr = this as *const Object as usize;
    if let Some(FocusMarker::ClickActivated(flag)) = focus_marker_of(ptr) {
        unsafe {
            // NSEventTypeLeftMouseDown = 1, NSEventTypeRightMouseDown = 3.
            let evt_type: i64 = msg_send![event, type];
            if evt_type == 1 || evt_type == 3 {
                flag.store(true, Ordering::SeqCst);
                let nil: *const Object = core::ptr::null();
                let recv: *mut Object = this as *const Object as *mut Object;
                let _: () = msg_send![recv, makeKeyAndOrderFront: nil];
            }
        }
    }
    unsafe {
        let cls: *const Class = msg_send![this as *const Object as *mut Object, class];
        if let Some(imps) = focus_origs_for(cls) {
            type Sig = extern "C" fn(&Object, Sel, *mut Object);
            let f: Sig = std::mem::transmute::<*const std::os::raw::c_void, Sig>(
                imps.send_event as *const std::os::raw::c_void,
            );
            f(this, sel, event);
        }
    }
}

#[cfg(target_os = "macos")]
extern "C" fn focus_resign_key_swizzled(this: &Object, sel: Sel) {
    let ptr = this as *const Object as usize;
    if let Some(FocusMarker::ClickActivated(flag)) = focus_marker_of(ptr) {
        flag.store(false, Ordering::SeqCst);
    }
    unsafe {
        let cls: *const Class = msg_send![this as *const Object as *mut Object, class];
        if let Some(imps) = focus_origs_for(cls) {
            type Sig = extern "C" fn(&Object, Sel);
            let f: Sig = std::mem::transmute::<*const std::os::raw::c_void, Sig>(
                imps.resign_key as *const std::os::raw::c_void,
            );
            f(this, sel);
        }
    }
}

/// Install canBecomeKeyWindow / canBecomeMainWindow / sendEvent: /
/// resignKeyWindow swizzles on `target_cls` if not already installed.
/// Returns true if the swizzle is in effect for this class chain.
#[cfg(target_os = "macos")]
unsafe fn ensure_focus_swizzle_installed(target_cls: *const Class) -> bool {
    use std::collections::HashMap;
    use std::os::raw::{c_char, c_void};
    use std::sync::Mutex;

    extern "C" {
        fn class_getInstanceMethod(cls: *const Class, sel: Sel) -> *mut c_void;
        fn class_addMethod(
            cls: *mut Class,
            sel: Sel,
            imp: *const c_void,
            types: *const c_char,
        ) -> objc::runtime::BOOL;
        fn method_getImplementation(method: *mut c_void) -> *const c_void;
        fn method_setImplementation(method: *mut c_void, imp: *const c_void) -> *const c_void;
    }

    let key = target_cls as usize;
    let map = FOCUS_SWIZZLE_ORIGS.get_or_init(|| Mutex::new(HashMap::new()));
    {
        let g = map.lock().unwrap();
        if g.contains_key(&key) {
            return true;
        }
    }

    // Helper: install one method, returning the IMP that was effective
    // for this class before our replacement (either inherited from a
    // superclass or directly on target_cls).
    unsafe fn install_one(
        target_cls: *const Class,
        sel: Sel,
        new_imp: *const c_void,
        types: *const c_char,
    ) -> *const c_void {
        let added = class_addMethod(target_cls as *mut Class, sel, new_imp, types);
        if added != objc::runtime::NO {
            // Method didn't exist on target_cls — was inherited.  The
            // pre-existing effective IMP is on a superclass.
            let m = class_getInstanceMethod(class_getSuperclass(target_cls), sel);
            if m.is_null() {
                core::ptr::null()
            } else {
                method_getImplementation(m)
            }
        } else {
            // Already on target_cls; replace and capture old IMP.
            let m = class_getInstanceMethod(target_cls, sel);
            if m.is_null() {
                core::ptr::null()
            } else {
                method_setImplementation(m, new_imp)
            }
        }
    }

    extern "C" {
        fn class_getSuperclass(cls: *const Class) -> *const Class;
    }

    let can_key_orig = install_one(
        target_cls,
        sel!(canBecomeKeyWindow),
        focus_can_become_key_swizzled as *const c_void,
        FOCUS_ENC_BOOL_NO_ARGS.as_ptr() as *const c_char,
    );
    let can_main_orig = install_one(
        target_cls,
        sel!(canBecomeMainWindow),
        focus_can_become_main_swizzled as *const c_void,
        FOCUS_ENC_BOOL_NO_ARGS.as_ptr() as *const c_char,
    );
    let send_event_orig = install_one(
        target_cls,
        sel!(sendEvent:),
        focus_send_event_swizzled as *const c_void,
        FOCUS_ENC_VOID_ID.as_ptr() as *const c_char,
    );
    let resign_orig = install_one(
        target_cls,
        sel!(resignKeyWindow),
        focus_resign_key_swizzled as *const c_void,
        FOCUS_ENC_VOID_NO_ARGS.as_ptr() as *const c_char,
    );

    let imps = FocusOrigImps {
        can_become_key: can_key_orig as usize,
        can_become_main: can_main_orig as usize,
        send_event: send_event_orig as usize,
        resign_key: resign_orig as usize,
    };
    map.lock().unwrap().insert(key, imps);
    true
}

#[cfg(target_os = "macos")]
fn focus_mark_window<F>(title: &str, make_marker: F)
where
    F: Fn() -> FocusMarker,
{
    use objc::runtime::Object as ObjcObject;
    use std::collections::HashMap;
    use std::sync::Mutex;
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let windows: *mut ObjcObject = msg_send![app, windows];
        if windows.is_null() {
            return;
        }
        let count: i64 = msg_send![windows, count];
        for i in 0..count {
            let w: *mut ObjcObject = msg_send![windows, objectAtIndex: i];
            if w.is_null() {
                continue;
            }
            let wt: *mut ObjcObject = msg_send![w, title];
            if wt.is_null() {
                continue;
            }
            let utf8: *const i8 = msg_send![wt, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let cur = std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string();
            if cur != title {
                continue;
            }
            let cls: *const Class = msg_send![w, class];
            if cls.is_null() {
                continue;
            }
            // The actual class might be NSKVONotifying_<X>; the swizzle
            // needs to land on <X> so KVO's lookup chain still finds
            // our IMP via inheritance.  Walk up to the first non-KVO
            // class.
            let target = focus_target_class(cls);
            if target.is_null() {
                continue;
            }
            if !ensure_focus_swizzle_installed(target) {
                continue;
            }
            let markers = FOCUS_MARKERS.get_or_init(|| Mutex::new(HashMap::new()));
            // `or_insert_with` skips the maker closure (and any
            // per-instance allocation it does for ClickActivated)
            // when the entry already exists — keeping every-frame
            // calls cheap and idempotent.
            markers
                .lock()
                .unwrap()
                .entry(w as usize)
                .or_insert_with(|| make_marker());
        }
    }
}

/// Return the non-KVO class for an NSWindow instance: walks up while
/// the class name starts with "NSKVONotifying_".  KVO's dynamic
/// subclasses derive from the real class, so the first non-KVO ancestor
/// is the class winit actually instantiated.
#[cfg(target_os = "macos")]
unsafe fn focus_target_class(mut cls: *const Class) -> *const Class {
    extern "C" {
        fn class_getSuperclass(cls: *const Class) -> *const Class;
        fn class_getName(cls: *const Class) -> *const i8;
    }
    while !cls.is_null() {
        let name_ptr = class_getName(cls);
        if name_ptr.is_null() {
            return cls;
        }
        let name = std::ffi::CStr::from_ptr(name_ptr).to_string_lossy();
        if !name.starts_with("NSKVONotifying_") {
            return cls;
        }
        cls = class_getSuperclass(cls);
    }
    cls
}

/// macOS: prevent an auxiliary NSWindow (matched by exact title) from
/// ever becoming the key / main window.  Used for the read-only Call
/// Stack panel — it accepts clicks but never needs keystrokes.
///
/// Idempotent — safe to call every frame.
#[cfg(target_os = "macos")]
pub fn mark_window_non_activating(title: &str) {
    focus_mark_window(title, || FocusMarker::NonActivating);
    // Best-effort: if AppKit had already promoted this window to key
    // before the marker was registered, push key status back to main.
    use objc::runtime::Object as ObjcObject;
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let windows: *mut ObjcObject = msg_send![app, windows];
        if windows.is_null() {
            return;
        }
        let count: i64 = msg_send![windows, count];
        for i in 0..count {
            let w: *mut ObjcObject = msg_send![windows, objectAtIndex: i];
            if w.is_null() {
                continue;
            }
            let wt: *mut ObjcObject = msg_send![w, title];
            if wt.is_null() {
                continue;
            }
            let utf8: *const i8 = msg_send![wt, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let cur = std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string();
            if cur != title {
                continue;
            }
            let key_window: *mut ObjcObject = msg_send![app, keyWindow];
            if key_window == w {
                let main_window: *mut ObjcObject = msg_send![app, mainWindow];
                if !main_window.is_null() && main_window != w {
                    let _: () =
                        msg_send![main_window, makeKeyAndOrderFront: core::ptr::null::<()>()];
                }
            }
        }
    }
}

/// macOS: install a "click-activated" marker on an NSWindow that needs
/// keystrokes only when the user has explicitly clicked it.  Used for:
///
///   * VT100 Console — typing flows into the running em6809 program
///     when Console is key.
///   * Breakpoints — the per-row condition `TextEdit` needs keystrokes
///     when the user is editing a condition; otherwise BP must not
///     reclaim key from main during eframe paint cycles.
///
/// Each marked window gets its own leaked `AtomicBool` so multiple
/// click-activated windows are independent (clicking one doesn't make
/// another eligible to become key).
///
/// Idempotent — safe to call every frame.  Per-instance allocation
/// only happens on the first call for each NSWindow.
#[cfg(target_os = "macos")]
pub fn mark_window_click_activated(title: &str) {
    focus_mark_window(title, || {
        let flag: &'static AtomicBool = Box::leak(Box::new(AtomicBool::new(true)));
        FocusMarker::ClickActivated(flag)
    });
}

#[cfg(target_os = "macos")]
pub fn disable_minimize_zoom_for_title(title: &str) {
    use objc::runtime::Object as ObjcObject;
    unsafe {
        let app: *mut ObjcObject = msg_send![class!(NSApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let windows: *mut ObjcObject = msg_send![app, windows];
        if windows.is_null() {
            return;
        }
        let count: i64 = msg_send![windows, count];
        for i in 0..count {
            let w: *mut ObjcObject = msg_send![windows, objectAtIndex: i];
            if w.is_null() {
                continue;
            }
            let wt: *mut ObjcObject = msg_send![w, title];
            if wt.is_null() {
                continue;
            }
            let utf8: *const i8 = msg_send![wt, UTF8String];
            if utf8.is_null() {
                continue;
            }
            let cur = std::ffi::CStr::from_ptr(utf8).to_string_lossy().to_string();
            if cur == title {
                // NSWindowButton indices: 0=Close, 1=Miniaturize, 2=Zoom
                let min_btn: *mut ObjcObject = msg_send![w, standardWindowButton: 1i64];
                if !min_btn.is_null() {
                    let _: () = msg_send![min_btn, setEnabled: false];
                }
                let zoom_btn: *mut ObjcObject = msg_send![w, standardWindowButton: 2i64];
                if !zoom_btn.is_null() {
                    let _: () = msg_send![zoom_btn, setEnabled: false];
                }
                // Leave Close [X] intact; don't alter style mask here
                break;
            }
        }
    }
}
