use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Prefs {
    pub general: Option<General>,
    pub console: Option<Console>,
    pub block: Option<Block>,
    pub gpio: Option<Gpio>,
    pub timer: Option<Timer>,
    pub mmu: Option<Mmu>,
    pub debug: Option<Debug>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct General {
    pub protect_vectors: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Console {
    pub enable: Option<bool>,
    pub base: Option<u16>,
    pub stdin: Option<bool>,
    pub echo: Option<bool>,
    pub out_path: Option<String>,
    pub flush_every: Option<usize>,
    pub flush_newline: Option<bool>,
    pub rx_watermark: Option<usize>,
    pub irq_hold: Option<u32>,
    pub tee_stderr: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Block {
    pub enable: Option<bool>,
    pub base: Option<u16>,
    pub file_path: Option<String>,
    pub irq_enable: Option<bool>,
    pub firq: Option<bool>,
    pub irq_hold: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Gpio {
    pub enable: Option<bool>,
    pub base: Option<u16>,
    pub bits: Option<u8>,
    pub on_color: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Timer {
    pub enable: Option<bool>,
    pub base: Option<u16>,
    pub reload: Option<u16>,
    pub irq_enable: Option<bool>,
    pub firq: Option<bool>,
    pub start: Option<bool>,
    pub rate_hz: Option<u32>,
    pub ips_est: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Mmu {
    pub enable: Option<bool>,
    pub regs_base: Option<u16>,
    pub phys_bytes: Option<usize>,
    pub sys_mode: Option<bool>,
    pub task: Option<u8>,
    pub prot: Option<u8>,
    pub protect_vectors: Option<bool>,
    pub log_maps: Option<bool>,
    pub map_frames: Option<Vec<(usize, u16)>>,
    pub attr_bytes: Option<Vec<(usize, u8)>>,
    pub map_frames_sys: Option<Vec<(usize, u16)>>,
    pub map_frames_user: Option<Vec<(usize, u16)>>,
    pub attr_bytes_sys: Option<Vec<(usize, u8)>>,
    pub attr_bytes_user: Option<Vec<(usize, u8)>>,
    pub preset_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Debug {
    pub console_log: Option<bool>,
    pub block_log: Option<bool>,
    pub gpio_log: Option<bool>,
    pub mmu_log: Option<bool>,
    pub settings_log: Option<bool>,
    pub show_settings_origins: Option<bool>,
}

fn config_path() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let base = std::env::var_os("HOME").map(PathBuf::from)?;
        let p = base
            .join("Library")
            .join("Application Support")
            .join("em6809");
        return Some(p.join("config.toml"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
            });
        let p = base.join("em6809");
        return Some(p.join("config.toml"));
    }
    #[cfg(windows)]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("AppData\\Roaming")
            });
        let p = base.join("em6809");
        return Some(p.join("config.toml"));
    }
}

pub fn load() -> Prefs {
    if let Some(path) = config_path() {
        eprintln!("prefs::load using path: {:?}", path);
        match fs::read_to_string(&path) {
            Ok(text) => match toml::from_str::<Prefs>(&text) {
                Ok(v) => return v,
                Err(e) => eprintln!("prefs::load parse error {path:?}: {e}"),
            },
            Err(e) => eprintln!("prefs::load read error {path:?}: {e}"),
        }
    }
    Prefs::default()
}

pub fn save(p: &Prefs) -> Result<(), String> {
    let Some(path) = config_path() else {
        return Err("No config path".into());
    };
    if let Some(dir) = path.parent() {
        if let Err(e) = fs::create_dir_all(dir) {
            return Err(format!("create_dir_all failed: {e}"));
        }
    }
    let text = toml::to_string_pretty(p).map_err(|e| format!("TOML serialize error: {e}"))?;
    fs::write(&path, text).map_err(|e| format!("write failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_toml_roundtrip() {
        let mut p = Prefs::default();
        p.console = Some(Console {
            enable: Some(true),
            base: Some(0xFF00),
            stdin: Some(false),
            echo: Some(true),
            out_path: Some("/tmp/out.txt".into()),
            flush_every: Some(4),
            flush_newline: Some(true),
            rx_watermark: Some(2),
            irq_hold: Some(0),
            tee_stderr: Some(false),
        });
        p.block = Some(Block {
            enable: Some(true),
            base: Some(0xFF20),
            file_path: Some("/tmp/disk.img".into()),
            irq_enable: Some(true),
            firq: Some(false),
            irq_hold: Some(8),
        });
        p.gpio = Some(Gpio {
            enable: Some(true),
            base: Some(0xFF40),
            bits: Some(8),
            on_color: Some(0x00FF00),
        });
        p.timer = Some(Timer {
            enable: Some(true),
            base: Some(0xFF10),
            reload: Some(0x1234),
            irq_enable: Some(true),
            firq: Some(false),
            start: Some(true),
            rate_hz: Some(1000),
            ips_est: Some(250_000),
        });
        let frames: Vec<(usize, u16)> = vec![(0, 0x003F)];
        let attrs: Vec<(usize, u8)> = vec![(0, 0x01)];
        p.mmu = Some(Mmu {
            enable: Some(true),
            regs_base: Some(0xFFA0),
            phys_bytes: Some(1 << 20),
            sys_mode: Some(true),
            task: Some(0),
            prot: Some(0x03),
            protect_vectors: Some(true),
            log_maps: Some(true),
            map_frames: None,
            attr_bytes: None,
            map_frames_sys: Some(frames.clone()),
            map_frames_user: Some(frames),
            attr_bytes_sys: Some(attrs.clone()),
            attr_bytes_user: Some(attrs),
            preset_name: Some("os9l2".into()),
        });

        let text = toml::to_string_pretty(&p).expect("serialize");
        let p2: Prefs = toml::from_str(&text).expect("deserialize");

        // spot-check a few values
        assert_eq!(p2.mmu.as_ref().unwrap().protect_vectors, Some(true));
        assert_eq!(p2.console.as_ref().unwrap().base, Some(0xFF00));
        assert_eq!(
            p2.block.as_ref().unwrap().file_path.as_deref(),
            Some("/tmp/disk.img")
        );
        assert_eq!(p2.gpio.as_ref().unwrap().on_color, Some(0x00FF00));
        assert_eq!(p2.timer.as_ref().unwrap().reload, Some(0x1234));
        assert_eq!(p2.mmu.as_ref().unwrap().regs_base, Some(0xFFA0));
        assert!(p2
            .mmu
            .as_ref()
            .unwrap()
            .map_frames_sys
            .as_ref()
            .unwrap()
            .contains(&(0, 0x003F)));
        assert!(p2
            .mmu
            .as_ref()
            .unwrap()
            .attr_bytes_sys
            .as_ref()
            .unwrap()
            .contains(&(0, 0x01)));
    }
}
