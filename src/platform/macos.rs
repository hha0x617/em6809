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
        let bp_item: *mut ObjcObject =
            msg_send![bp_item, initWithTitle: nsstring("Breakpoints") action: sel!(em_toggleBp:) keyEquivalent: nsstring("")];
        let _: () = msg_send![bp_item, setTarget: target];
        let _: () = msg_send![view_menu, addItem: bp_item];

        let cs_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let cs_item: *mut ObjcObject =
            msg_send![cs_item, initWithTitle: nsstring("Call Stack") action: sel!(em_toggleCs:) keyEquivalent: nsstring("")];
        let _: () = msg_send![cs_item, setTarget: target];
        let _: () = msg_send![view_menu, addItem: cs_item];

        let top_item: *mut ObjcObject = msg_send![class!(NSMenuItem), alloc];
        let top_item: *mut ObjcObject =
            msg_send![top_item, initWithTitle: view_title action: core::ptr::null::<()>() keyEquivalent: nsstring("")];
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
