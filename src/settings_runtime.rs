use crate::prefs;
use crate::regview;
use crate::settings_worker;
use em6809::bus::{self, Bus};
use em6809::io::IoBus;
use em6809::mmu::Mc6829;
use std::path::Path;

pub fn apply_settings_runtime(
    viewer: Option<&regview::Viewer>,
    bus: &mut dyn bus::Bus,
    payload: &settings_worker::SettingsPayload,
) -> bool {
    let settings = &payload.settings;
    em6809::io::set_console_log(settings.debug_console_log);
    em6809::io::set_block_log(settings.debug_block_log);
    em6809::io::set_gpio_log(settings.debug_gpio_log);
    regview::gui::set_show_settings_origins(settings.debug_show_origins);
    crate::settings_native::set_settings_debug_log(settings.debug_settings_log);
    let console_base = parse_settings_u16(&settings.console_base_hex).unwrap_or(0xFF00);
    let console_out_path = if settings.console_out_path.trim().is_empty() {
        None
    } else {
        Some(settings.console_out_path.clone())
    };
    let console_flush_every = parse_settings_usize(&settings.console_flush_every);
    let configure_console = |iobus: &mut dyn core::any::Any| {
        if let Some(io_mem) = iobus.downcast_mut::<IoBus<bus::Memory>>() {
            apply_console_config(
                io_mem,
                settings.use_console,
                console_base,
                console_out_path.as_deref(),
                settings.console_echo,
                console_flush_every,
            );
        } else if let Some(io_mmu) = iobus.downcast_mut::<IoBus<Mc6829>>() {
            apply_console_config(
                io_mmu,
                settings.use_console,
                console_base,
                console_out_path.as_deref(),
                settings.console_echo,
                console_flush_every,
            );
        }
    };
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<bus::Memory>>() {
        apply_console_config(
            io_mem,
            settings.use_console,
            console_base,
            console_out_path.as_deref(),
            settings.console_echo,
            console_flush_every,
        );
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_console_config(
            io_mmu,
            settings.use_console,
            console_base,
            console_out_path.as_deref(),
            settings.console_echo,
            console_flush_every,
        );
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        configure_console(inner_any);
    }

    let block_base = parse_settings_u16(&settings.block_base_hex).unwrap_or(0xFF20);
    let block_path = if settings.block_file_path.trim().is_empty() {
        None
    } else {
        Some(settings.block_file_path.clone())
    };
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<bus::Memory>>() {
        apply_block_config(
            io_mem,
            settings.use_block,
            block_base,
            block_path.as_deref(),
        );
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_block_config(
            io_mmu,
            settings.use_block,
            block_base,
            block_path.as_deref(),
        );
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<bus::Memory>>() {
            apply_block_config(
                io_mem,
                settings.use_block,
                block_base,
                block_path.as_deref(),
            );
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            apply_block_config(
                io_mmu,
                settings.use_block,
                block_base,
                block_path.as_deref(),
            );
        }
    }
    if let Some(view) = viewer {
        sync_block_from_bus(bus, view);
    }

    let gpio_base = parse_settings_u16(&settings.gpio_base_hex).unwrap_or(0xFF40);
    let gpio_bits = parse_settings_u8(&settings.gpio_bits).unwrap_or(8);
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<bus::Memory>>() {
        apply_gpio_config(io_mem, settings.use_gpio, gpio_base, gpio_bits);
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_gpio_config(io_mmu, settings.use_gpio, gpio_base, gpio_bits);
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<bus::Memory>>() {
            apply_gpio_config(io_mem, settings.use_gpio, gpio_base, gpio_bits);
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            apply_gpio_config(io_mmu, settings.use_gpio, gpio_base, gpio_bits);
        }
    }
    if let Some(view) = viewer {
        sync_gpio_from_bus(bus, view);
    }

    let timer_base = parse_settings_u16(&settings.timer_base_hex).unwrap_or(0xFF10);
    let timer_reload = parse_settings_u16(&settings.timer_reload_hex);
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<bus::Memory>>() {
        apply_timer_config(
            io_mem,
            settings.use_timer,
            timer_base,
            timer_reload,
            settings.timer_irq_enable,
            settings.timer_firq,
            settings.timer_start,
            viewer,
        );
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        apply_timer_config(
            io_mmu,
            settings.use_timer,
            timer_base,
            timer_reload,
            settings.timer_irq_enable,
            settings.timer_firq,
            settings.timer_start,
            viewer,
        );
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<bus::Memory>>() {
            apply_timer_config(
                io_mem,
                settings.use_timer,
                timer_base,
                timer_reload,
                settings.timer_irq_enable,
                settings.timer_firq,
                settings.timer_start,
                viewer,
            );
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            apply_timer_config(
                io_mmu,
                settings.use_timer,
                timer_base,
                timer_reload,
                settings.timer_irq_enable,
                settings.timer_firq,
                settings.timer_start,
                viewer,
            );
        }
    }

    let mut maps_sys: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in settings.mmu_map_frames_sys.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            maps_sys.push((idx, v));
        }
    }
    let mut maps_user: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in settings.mmu_map_frames_user.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            maps_user.push((idx, v));
        }
    }
    let mut attrs_sys: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in settings.mmu_attr_bytes_sys.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            attrs_sys.push((idx, (v & 0xFF) as u8));
        }
    }
    let mut attrs_user: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in settings.mmu_attr_bytes_user.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            attrs_user.push((idx, (v & 0xFF) as u8));
        }
    }
    let task_opt = parse_settings_usize(&settings.mmu_task).map(|v| (v as u8) & 0x07);
    let prot_opt = parse_settings_u16(&settings.mmu_prot_hex).map(|v| (v & 0xFF) as u8);
    set_mmu_log(bus, settings.debug_mmu_log);
    apply_mmu_config(
        bus,
        settings.mmu_mode_sys,
        task_opt,
        prot_opt,
        settings.mmu_log_maps,
        &maps_sys,
        &maps_user,
        &attrs_sys,
        &attrs_user,
    );

    if let Some(view) = viewer {
        let push_map = |mmu: &mut Mc6829| {
            let (mut map_sys, mut map_user, sys, task) = mmu.snapshot_maps();
            for (idx, entry) in settings.mmu_attr_bytes_sys.iter().enumerate() {
                if let Some(v) = parse_settings_u16(entry) {
                    let attr = (v & 0xFF) as u16;
                    map_sys[idx] = (map_sys[idx] & 0x00FF) | (attr << 8);
                }
            }
            for (idx, entry) in settings.mmu_attr_bytes_user.iter().enumerate() {
                if let Some(v) = parse_settings_u16(entry) {
                    let attr = (v & 0xFF) as u16;
                    map_user[idx] = (map_user[idx] & 0x00FF) | (attr << 8);
                }
            }
            view.sync_mmu(sys, task, map_sys, map_user);
        };
        if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
            let inner = io_mmu.inner_any_mut();
            if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
                push_map(mmu);
            }
        } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
            push_map(mmu);
        } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
            let inner_any = wt.inner_any_mut();
            if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
                let inner2 = io_mmu.inner_any_mut();
                if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                    push_map(mmu);
                }
            } else if let Some(mmu) = inner_any.downcast_mut::<Mc6829>() {
                push_map(mmu);
            }
        }
        view.set_gpio_on_color(payload.gpio_on_color);
    }
    true
}

pub fn persist_settings_snapshot(state: &regview::SettingsState, gpio_on_color: u32) {
    let mut p = prefs::Prefs::default();
    p.general = None;
    p.console = Some(prefs::Console {
        enable: Some(state.use_console),
        base: parse_settings_u16(&state.console_base_hex),
        stdin: Some(state.console_stdin),
        echo: Some(state.console_echo),
        out_path: if state.console_out_path.is_empty() {
            None
        } else {
            Some(state.console_out_path.clone())
        },
        flush_every: parse_settings_usize(&state.console_flush_every),
        flush_newline: Some(state.console_flush_newline),
        rx_watermark: parse_settings_usize(&state.console_rx_wm),
        irq_hold: parse_settings_u32(&state.console_irq_hold),
        tee_stderr: Some(state.console_tee_stderr),
    });
    p.block = Some(prefs::Block {
        enable: Some(state.use_block),
        base: parse_settings_u16(&state.block_base_hex),
        file_path: if state.block_file_path.is_empty() {
            None
        } else {
            Some(state.block_file_path.clone())
        },
        irq_enable: Some(state.block_irq_enable),
        firq: Some(state.block_firq),
        irq_hold: parse_settings_u32(&state.block_irq_hold),
    });
    p.gpio = Some(prefs::Gpio {
        enable: Some(state.use_gpio),
        base: parse_settings_u16(&state.gpio_base_hex),
        bits: parse_settings_u8(&state.gpio_bits),
        on_color: Some(gpio_on_color),
    });
    p.timer = Some(prefs::Timer {
        enable: Some(state.use_timer),
        base: parse_settings_u16(&state.timer_base_hex),
        reload: parse_settings_u16(&state.timer_reload_hex),
        irq_enable: Some(state.timer_irq_enable),
        firq: Some(state.timer_firq),
        start: Some(state.timer_start),
        rate_hz: parse_settings_u32(&state.timer_rate_hz),
        ips_est: parse_settings_usize(&state.timer_ips).map(|v| v as u64),
    });
    let mut frames_sys: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in state.mmu_map_frames_sys.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            frames_sys.push((idx, v & 0x00FF));
        }
    }
    let mut frames_user: Vec<(usize, u16)> = Vec::new();
    for (idx, entry) in state.mmu_map_frames_user.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            frames_user.push((idx, v & 0x00FF));
        }
    }
    let mut attrs_sys: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in state.mmu_attr_bytes_sys.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            attrs_sys.push((idx, (v & 0xFF) as u8));
        }
    }
    let mut attrs_user: Vec<(usize, u8)> = Vec::new();
    for (idx, entry) in state.mmu_attr_bytes_user.iter().enumerate() {
        if let Some(v) = parse_settings_u16(entry) {
            attrs_user.push((idx, (v & 0xFF) as u8));
        }
    }
    p.mmu = Some(prefs::Mmu {
        enable: Some(state.use_mmu),
        regs_base: parse_settings_u16(&state.mmu_regs_base_hex),
        phys_bytes: parse_settings_usize(&state.mmu_phys_bytes),
        sys_mode: Some(state.mmu_mode_sys),
        task: parse_settings_u8(&state.mmu_task),
        prot: parse_settings_u16(&state.mmu_prot_hex).map(|v| (v & 0xFF) as u8),
        protect_vectors: Some(state.protect_vectors),
        log_maps: Some(state.mmu_log_maps),
        map_frames: None,
        attr_bytes: None,
        map_frames_sys: Some(frames_sys),
        map_frames_user: Some(frames_user),
        attr_bytes_sys: Some(attrs_sys),
        attr_bytes_user: Some(attrs_user),
        preset_name: if state.mmu_preset_name.is_empty() {
            None
        } else {
            Some(state.mmu_preset_name.clone())
        },
    });
    p.debug = Some(prefs::Debug {
        console_log: Some(state.debug_console_log),
        block_log: Some(state.debug_block_log),
        gpio_log: Some(state.debug_gpio_log),
        mmu_log: Some(state.debug_mmu_log),
        settings_log: Some(state.debug_settings_log),
        show_settings_origins: Some(state.debug_show_origins),
    });
    if let Err(e) = prefs::save(&p) {
        eprintln!("prefs::save failed: {e}");
    }
}

pub fn publish_settings_baseline(state: &regview::SettingsState, gpio_on_color: u32) {
    let origins_vec: Vec<(String, u8)> =
        if let Some((_, origins_map, _)) = regview::get_latest_settings_baseline() {
            let mut entries: Vec<(String, u8)> = origins_map.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            entries
        } else {
            Vec::new()
        };
    let _ = regview::push_global_command(regview::Command::UiSetSettingsBaseline(
        state.clone(),
        origins_vec,
        Some(gpio_on_color),
    ));
}

pub fn send_settings_ack(instance_id: u64) {
    let _ = regview::push_global_command(regview::Command::UiAckSettings { instance_id });
}

pub fn sync_gpio_from_bus(bus: &mut dyn bus::Bus, viewer: &regview::Viewer) {
    let mut synced = false;
    if let Some(iobus) = bus.as_any_mut().downcast_mut::<IoBus<bus::Memory>>() {
        iobus.with_gpio_mut_opt(|g| {
            let (out, dir, bits) = g.get_state();
            regview::publish_gpio_snapshot(true, bits, out, dir);
            viewer.sync_gpio(true, bits, out, dir);
            synced = true;
        });
    } else if let Some(iobus) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        iobus.with_gpio_mut_opt(|g| {
            let (out, dir, bits) = g.get_state();
            regview::publish_gpio_snapshot(true, bits, out, dir);
            viewer.sync_gpio(true, bits, out, dir);
            synced = true;
        });
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<bus::Memory>>() {
            io_mem.with_gpio_mut_opt(|g| {
                let (out, dir, bits) = g.get_state();
                regview::publish_gpio_snapshot(true, bits, out, dir);
                viewer.sync_gpio(true, bits, out, dir);
                synced = true;
            });
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            io_mmu.with_gpio_mut_opt(|g| {
                let (out, dir, bits) = g.get_state();
                regview::publish_gpio_snapshot(true, bits, out, dir);
                viewer.sync_gpio(true, bits, out, dir);
                synced = true;
            });
        }
    }
    if !synced {
        regview::publish_gpio_snapshot(false, 0, 0, 0);
        viewer.sync_gpio(false, 0, 0, 0);
    }
}

pub fn sync_block_from_bus(bus: &mut dyn bus::Bus, viewer: &regview::Viewer) {
    fn block_info_from_iobus<B: bus::Bus>(
        iobus: &mut IoBus<B>,
    ) -> Option<(u16, Option<String>, u8, u8, u8)> {
        let mut info: Option<(u16, Option<String>, u8, u8, u8)> = None;
        iobus.with_block_mut_opt(|blk| {
            info = Some((
                blk.base,
                blk.backing_file().map(|s| s.to_string()),
                blk.last_cmd(),
                blk.last_data(),
                blk.status(),
            ));
        });
        info
    }
    let mut info: Option<(u16, Option<String>, u8, u8, u8)> = None;
    if let Some(io_mem) = bus.as_any_mut().downcast_mut::<IoBus<bus::Memory>>() {
        info = block_info_from_iobus(io_mem);
    } else if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        info = block_info_from_iobus(io_mmu);
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mem) = inner_any.downcast_mut::<IoBus<bus::Memory>>() {
            info = block_info_from_iobus(io_mem);
        } else if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            info = block_info_from_iobus(io_mmu);
        }
    }
    if let Some((base, path_opt, last_cmd, last_data, status)) = info {
        let online = path_opt
            .as_ref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);
        viewer.sync_block_status(
            true,
            Some(base),
            path_opt,
            online,
            last_cmd,
            last_data,
            status,
        );
    } else {
        viewer.sync_block_status(false, None, None, false, 0, 0, 0);
    }
}

pub fn parse_settings_u16(s: &str) -> Option<u16> {
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

fn parse_settings_usize(s: &str) -> Option<usize> {
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

fn parse_settings_u32(s: &str) -> Option<u32> {
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

fn parse_settings_u8(s: &str) -> Option<u8> {
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
        .map(|v| (v & 0xFF) as u8)
}

fn apply_console_config<B: bus::Bus>(
    iobus: &mut IoBus<B>,
    enable: bool,
    base: u16,
    out_path: Option<&str>,
    echo: bool,
    flush_every: Option<usize>,
) {
    iobus.ensure_console(enable, base);
    iobus.with_console_mut_opt(|c| {
        if let Some(path) = out_path {
            c.set_out_file(path);
        }
        c.set_local_echo(echo);
        if let Some(n) = flush_every {
            c.set_flush_every(n);
        }
    });
}

fn apply_block_config<B: bus::Bus>(
    iobus: &mut IoBus<B>,
    enable: bool,
    base: u16,
    file_path: Option<&str>,
) {
    iobus.ensure_block(enable, base);
    if let Some(path) = file_path {
        iobus.with_block_mut_opt(|blk| blk.set_backing_file(path));
    }
}

fn apply_gpio_config<B: bus::Bus>(iobus: &mut IoBus<B>, enable: bool, base: u16, bits: u8) {
    iobus.ensure_gpio(enable, base, bits);
}

fn apply_timer_config<B: bus::Bus>(
    iobus: &mut IoBus<B>,
    enable: bool,
    base: u16,
    reload: Option<u16>,
    irq_enable: bool,
    firq: bool,
    start: bool,
    viewer: Option<&regview::Viewer>,
) {
    iobus.ensure_timer(enable, base);
    if let Some(r) = reload {
        iobus.with_timer_mut_opt(|t| t.set_reload(r));
    }
    iobus.with_timer_mut_opt(|t| {
        t.set_irq_enable(irq_enable);
        t.set_firq(firq);
        if start {
            t.start();
        } else {
            t.stop();
        }
    });
    if let Some(view) = viewer {
        if enable {
            iobus.with_timer_mut_opt(|t| {
                let (run, _irq_en, _firq, pending) = t.get_state();
                view.sync_timer(true, pending, run);
            });
        } else {
            view.sync_timer(false, false, false);
        }
    }
}

fn apply_mmu_config(
    bus: &mut dyn bus::Bus,
    sys_mode: bool,
    task: Option<u8>,
    prot: Option<u8>,
    log_maps: bool,
    maps_sys: &[(usize, u16)],
    maps_user: &[(usize, u16)],
    attrs_sys: &[(usize, u8)],
    attrs_user: &[(usize, u8)],
) {
    let apply = |mmu: &mut Mc6829| {
        mmu.set_log_maps(log_maps);
        if let Some(t) = task {
            mmu.write8(mmu.regs_base + 0x10, t & 0x07);
        }
        if let Some(p) = prot {
            mmu.write8(mmu.regs_base + 0x12, p);
        }
        mmu.write8(mmu.regs_base + 0x11, 0x01);
        for (pg, fr) in maps_sys {
            mmu.set_map_entry(*pg, *fr);
        }
        for (pg, at) in attrs_sys {
            let off = 0x18u16 + (*pg as u16);
            mmu.write8(mmu.regs_base + off, *at);
        }
        mmu.write8(mmu.regs_base + 0x11, 0x00);
        for (pg, fr) in maps_user {
            mmu.set_map_entry(*pg, *fr);
        }
        for (pg, at) in attrs_user {
            let off = 0x18u16 + (*pg as u16);
            mmu.write8(mmu.regs_base + off, *at);
        }
        mmu.write8(mmu.regs_base + 0x11, if sys_mode { 1 } else { 0 });
    };
    if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        let inner = io_mmu.inner_any_mut();
        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
            apply(mmu);
            return;
        }
    } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
        apply(mmu);
        return;
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            let inner2 = io_mmu.inner_any_mut();
            if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                apply(mmu);
                return;
            }
        } else if let Some(mmu) = inner_any.downcast_mut::<Mc6829>() {
            apply(mmu);
            return;
        }
    }
}

fn set_mmu_log(bus: &mut dyn bus::Bus, on: bool) {
    if let Some(io_mmu) = bus.as_any_mut().downcast_mut::<IoBus<Mc6829>>() {
        let inner = io_mmu.inner_any_mut();
        if let Some(mmu) = inner.downcast_mut::<Mc6829>() {
            mmu.set_log_maps(on);
            return;
        }
    } else if let Some(mmu) = bus.as_any_mut().downcast_mut::<Mc6829>() {
        mmu.set_log_maps(on);
        return;
    } else if let Some(wt) = bus.as_any_mut().downcast_mut::<bus::WriteTrack>() {
        let inner_any = wt.inner_any_mut();
        if let Some(io_mmu) = inner_any.downcast_mut::<IoBus<Mc6829>>() {
            let inner2 = io_mmu.inner_any_mut();
            if let Some(mmu) = inner2.downcast_mut::<Mc6829>() {
                mmu.set_log_maps(on);
                return;
            }
        } else if let Some(mmu) = inner_any.downcast_mut::<Mc6829>() {
            mmu.set_log_maps(on);
            return;
        }
    }
}
