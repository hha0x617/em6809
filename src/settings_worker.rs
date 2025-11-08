use crate::regview::SettingsState;
use crate::prefs;
use once_cell::sync::OnceCell;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// Payload shared across worker commands so runtime updates and persistence
/// can be performed even when the emulator is running.
#[derive(Clone, Debug)]
pub struct SettingsPayload {
    pub settings: SettingsState,
    pub gpio_on_color: u32,
    pub instance_id: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub enum CloseReason {
    Cancel,
    CloseRequested,
    FatalError,
}

#[derive(Clone, Debug)]
pub enum WorkerCommand {
    Apply(SettingsPayload),
    #[allow(dead_code)]
    SavePrefs(SettingsPayload),
    #[allow(dead_code)]
    CloseNative {
        reason: CloseReason,
        instance_id: u64,
    },
    #[allow(dead_code)]
    RefreshBaseline,
}

type Queue = Arc<Mutex<VecDeque<WorkerCommand>>>;

static WORKER_QUEUE: OnceCell<Queue> = OnceCell::new();
static ACTIVE_INSTANCE: OnceCell<AtomicU64> = OnceCell::new();
static NEXT_INSTANCE: OnceCell<AtomicU64> = OnceCell::new();

fn queue() -> &'static Queue {
    WORKER_QUEUE.get_or_init(|| Arc::new(Mutex::new(VecDeque::new())))
}

fn active_instance() -> &'static AtomicU64 {
    ACTIVE_INSTANCE.get_or_init(|| AtomicU64::new(0))
}

fn next_instance_counter() -> &'static AtomicU64 {
    NEXT_INSTANCE.get_or_init(|| AtomicU64::new(1))
}

/// Returns a monotonically increasing dialog instance ID.
pub fn next_dialog_instance() -> u64 {
    next_instance_counter().fetch_add(1, Ordering::SeqCst)
}

/// Update the most recent dialog instance ID so stale commands from older dialogs can be dropped.
pub fn set_active_instance(instance_id: u64) {
    active_instance().store(instance_id, Ordering::SeqCst);
}

/// Push a worker command into the queue shared by UI threads and the emulator worker.
pub fn push_worker_command(cmd: WorkerCommand) {
    if let Ok(mut guard) = queue().lock() {
        // Persist immediately for SavePrefs so settings survive even if worker loop
        // fails to drain before exit (e.g., native dialogs on macOS/Linux).
        if let WorkerCommand::SavePrefs(ref payload) = cmd {
            persist_settings_snapshot(&payload.settings, payload.gpio_on_color);
        }
        guard.push_back(cmd);
    }
}

/// Drain queued commands and invoke the supplied handler for each entry.
pub fn drain_worker_commands<F>(mut handler: F)
where
    F: FnMut(WorkerCommand),
{
    loop {
        let cmd_opt = {
            if let Ok(mut guard) = queue().lock() {
                guard.pop_front()
            } else {
                None
            }
        };
        match cmd_opt {
            Some(cmd) => handler(cmd),
            None => break,
        }
    }
}

/// Returns true if the given instance_id is older than the latest known dialog.
pub fn is_stale_instance(instance_id: u64) -> bool {
    instance_id < active_instance().load(Ordering::SeqCst)
}

pub fn current_instance_id() -> u64 {
    active_instance().load(Ordering::SeqCst)
}

fn parse_u16_any(s: &str) -> Option<u16> {
    let sl = s.trim();
    if sl.is_empty() {
        return None;
    }
    let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
        (2, rest)
    } else {
        (10, sl)
    };
    u32::from_str_radix(digits, base)
        .ok()
        .and_then(|v| if v <= 0xFFFF { Some(v as u16) } else { None })
}

fn parse_u32_any(s: &str) -> Option<u32> {
    let sl = s.trim();
    if sl.is_empty() {
        return None;
    }
    let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
        (2, rest)
    } else {
        (10, sl)
    };
    u32::from_str_radix(digits, base).ok()
}

fn parse_usize_any(s: &str) -> Option<usize> {
    let sl = s.trim();
    if sl.is_empty() {
        return None;
    }
    let (base, digits) = if let Some(rest) = sl.strip_prefix("0x").or(sl.strip_prefix("0X")) {
        (16, rest)
    } else if let Some(rest) = sl.strip_prefix("0o").or(sl.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = sl.strip_prefix("0b").or(sl.strip_prefix("0B")) {
        (2, rest)
    } else {
        (10, sl)
    };
    usize::from_str_radix(digits, base).ok()
}

pub fn persist_settings_snapshot(state: &SettingsState, gpio_on_color: u32) {
    let mut p = prefs::Prefs::default();
    p.general = Some(prefs::General {
        protect_vectors: Some(state.protect_vectors),
    });
    p.console = Some(prefs::Console {
        enable: Some(state.use_console),
        base: parse_u16_any(&state.console_base_hex),
        stdin: Some(state.console_stdin),
        echo: Some(state.console_echo),
        out_path: if state.console_out_path.trim().is_empty() {
            None
        } else {
            Some(state.console_out_path.clone())
        },
        flush_every: parse_usize_any(&state.console_flush_every),
        flush_newline: Some(state.console_flush_newline),
        rx_watermark: parse_usize_any(&state.console_rx_wm),
        irq_hold: parse_u32_any(&state.console_irq_hold),
        tee_stderr: Some(state.console_tee_stderr),
    });
    p.block = Some(prefs::Block {
        enable: Some(state.use_block),
        base: parse_u16_any(&state.block_base_hex),
        file_path: if state.block_file_path.trim().is_empty() {
            None
        } else {
            Some(state.block_file_path.clone())
        },
        irq_enable: Some(state.block_irq_enable),
        firq: Some(state.block_firq),
        irq_hold: parse_u32_any(&state.block_irq_hold),
    });
    p.gpio = Some(prefs::Gpio {
        enable: Some(state.use_gpio),
        base: parse_u16_any(&state.gpio_base_hex),
        bits: parse_usize_any(&state.gpio_bits).map(|v| (v & 0xFF) as u8),
        on_color: Some(gpio_on_color),
    });
    p.timer = Some(prefs::Timer {
        enable: Some(state.use_timer),
        base: parse_u16_any(&state.timer_base_hex),
        reload: parse_u16_any(&state.timer_reload_hex),
        irq_enable: Some(state.timer_irq_enable),
        firq: Some(state.timer_firq),
        start: Some(state.timer_start),
        rate_hz: parse_u32_any(&state.timer_rate_hz),
        ips_est: parse_usize_any(&state.timer_ips).map(|v| v as u64),
    });
    let mut frames: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in state.mmu_map_frames.iter().enumerate() {
        if let Some(v) = parse_u16_any(entry) {
            frames.push((idx, v));
        }
    }
    let mut attrs: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in state.mmu_attr_bytes.iter().enumerate() {
        if let Some(v) = parse_u16_any(entry) {
            attrs.push((idx, (v & 0xFF) as u8));
        }
    }
    p.mmu = Some(prefs::Mmu {
        enable: Some(state.use_mmu),
        regs_base: parse_u16_any(&state.mmu_regs_base_hex),
        phys_bytes: parse_usize_any(&state.mmu_phys_bytes),
        sys_mode: Some(state.mmu_mode_sys),
        task: parse_usize_any(&state.mmu_task).map(|v| (v & 0xFF) as u8),
        prot: parse_u16_any(&state.mmu_prot_hex).map(|v| (v & 0xFF) as u8),
        protect_vectors: None,
        log_maps: Some(state.mmu_log_maps),
        map_frames: Some(frames),
        attr_bytes: Some(attrs),
        preset_name: if state.mmu_preset_name.trim().is_empty() {
            None
        } else {
            Some(state.mmu_preset_name.clone())
        },
    });
    if let Err(e) = prefs::save(&p) {
        eprintln!("prefs::save failed: {e}");
    }
}
