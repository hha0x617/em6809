use i18n_embed::fluent::FluentLanguageLoader;
use once_cell::sync::Lazy;
use rust_embed::RustEmbed;
use unic_langid::LanguageIdentifier;

pub static LOADER: Lazy<FluentLanguageLoader> = Lazy::new(|| {
    let fallback: LanguageIdentifier = "en".parse().unwrap_or_else(|_| "en".parse().unwrap());
    FluentLanguageLoader::new("em6809", fallback)
});

#[derive(RustEmbed)]
#[folder = "locales"]
pub struct Locales;

fn parse_lang(tag: &str) -> Option<LanguageIdentifier> {
    tag.parse::<LanguageIdentifier>().ok()
}

fn detect_japanese_locale_fallback() -> bool {
    // Check common POSIX environment variables
    for key in ["LC_ALL", "LC_MESSAGES", "LANG"].iter() {
        if let Ok(v) = std::env::var(key) {
            if v.to_lowercase().starts_with("ja") {
                return true;
            }
        }
    }
    // Platform-specific fallback checks
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetUserDefaultLocaleName(lpLocaleName: *mut u16, cchLocaleName: i32) -> i32;
        }
        let mut buf = [0u16; 85];
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 0 {
            if let Ok(s) = String::from_utf16(&buf[..(n as usize - 1)]) {
                if s.to_lowercase().starts_with("ja") {
                    return true;
                }
            }
        }
    }
    // On macOS/Linux, rely on environment variables above.
    false
}

pub fn init_from_env() {
    // Priority 1: EM6809_LANG if provided
    let requested: Vec<LanguageIdentifier> = if let Ok(v) = std::env::var("EM6809_LANG") {
        let t = v.to_lowercase();
        if t.starts_with("ja") {
            vec![parse_lang("ja").unwrap()]
        } else if t.starts_with("en") {
            vec![parse_lang("en").unwrap()]
        } else {
            // Unknown tag -> fall back to English
            vec![parse_lang("en").unwrap()]
        }
    } else {
        // Priority 2: Fallback to environment/OS detection (same policy as GUI)
        if detect_japanese_locale_fallback() {
            vec![parse_lang("ja").unwrap()]
        } else {
            vec![parse_lang("en").unwrap()]
        }
    };
    let _ = i18n_embed::select(&*LOADER, &Locales, &requested);
}
