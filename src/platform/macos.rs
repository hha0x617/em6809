#![allow(unexpected_cfgs)]

#[cfg(target_os = "macos")]
use objc::runtime::Object;
#[cfg(target_os = "macos")]
use objc::{class, msg_send, sel, sel_impl};

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
    use objc::runtime::{Class, Object as ObjcObject, Sel};
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
