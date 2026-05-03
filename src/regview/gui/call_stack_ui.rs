// Call-stack window — a real OS-level child window (eframe viewport
// API), same pattern as the VT100 console and the breakpoints
// window. Read-only view of the shadow call stack, rendered
// newest-first so the active frame sits at the top.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use once_cell::sync::OnceCell;

use super::{CallFrameInfo, Command};

static CS_VISIBLE: OnceCell<AtomicBool> = OnceCell::new();
static CS_CLOSED: OnceCell<AtomicBool> = OnceCell::new();

pub(crate) fn call_stack_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("em6809_call_stack_window")
}

fn update_visible(desired: bool) -> bool {
    let flag = CS_VISIBLE.get_or_init(|| AtomicBool::new(desired));
    let prev = flag.swap(desired, Ordering::SeqCst);
    prev != desired
}

fn set_visible(ctx: &egui::Context, visible: bool) {
    update_visible(visible);
    ctx.send_viewport_cmd_to(
        call_stack_viewport_id(),
        egui::ViewportCommand::Visible(visible),
    );
}

fn note_closed() {
    if let Some(flag) = CS_CLOSED.get() {
        flag.store(true, Ordering::SeqCst);
    } else {
        let _ = CS_CLOSED.set(AtomicBool::new(true));
    }
}

pub(crate) fn take_closed() -> bool {
    CS_CLOSED
        .get()
        .map(|f| f.swap(false, Ordering::SeqCst))
        .unwrap_or(false)
}

fn render_call_stack(
    ui: &mut egui::Ui,
    call_stack: &Arc<Mutex<Vec<CallFrameInfo>>>,
    cmds: &Arc<Mutex<Vec<Command>>>,
) {
    let frames: Vec<CallFrameInfo> = call_stack.lock().map(|g| g.clone()).unwrap_or_default();

    if frames.is_empty() {
        ui.weak("No active calls.");
        ui.add_space(4.0);
        ui.label(
            "Frames are pushed by BSR/JSR/SWI and by IRQ/FIRQ/NMI \
             entry, popped by RTS/RTI.",
        );
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("call_stack_window_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized(
                    [28.0, 0.0],
                    egui::Label::new(egui::RichText::new("#").monospace().strong()),
                );
                ui.add_sized(
                    [46.0, 0.0],
                    egui::Label::new(egui::RichText::new("Kind").monospace().strong()),
                );
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("Call").monospace().strong()),
                );
                ui.add_sized([22.0, 0.0], egui::Label::new(""));
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("Target").monospace().strong()),
                );
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("Return").monospace().strong()),
                );
                ui.add_sized(
                    [62.0, 0.0],
                    egui::Label::new(egui::RichText::new("S").monospace().strong()),
                );
            });
            ui.separator();

            let total = frames.len();
            for (rev_idx, frame) in frames.iter().rev().enumerate() {
                let depth = total - rev_idx;
                ui.horizontal(|ui| {
                    ui.add_sized(
                        [28.0, 0.0],
                        egui::Label::new(egui::RichText::new(format!("{}", depth)).monospace()),
                    );
                    ui.add_sized(
                        [46.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(frame.kind.label()).monospace().strong(),
                        ),
                    );
                    // Each address cell is clickable.  Same
                    // default-framed `Button` shape as the BP delete
                    // button — the only `clicked()` pattern we know
                    // works on Windows.  Manual timestamp double-
                    // click detection bypasses egui's flaky
                    // `double_clicked()`.  Temporary `eprintln!`s
                    // confirm clicks reach us; remove once
                    // verified.
                    let dc_threshold = 0.4_f64; // seconds
                    let now = ui.input(|i| i.time);

                    let call_resp = ui.add_sized(
                        [62.0, 0.0],
                        egui::Button::new(
                            egui::RichText::new(format!("${:04X}", frame.call_site)).monospace(),
                        ),
                    );
                    if call_resp.clicked() {
                        eprintln!(
                            "[cs-click] call_site=${:04X} depth={}",
                            frame.call_site, depth
                        );
                        let mem_id = egui::Id::new(("cs_call_dc", depth));
                        let prev: f64 = ui.ctx().memory(|m| m.data.get_temp(mem_id).unwrap_or(0.0));
                        if prev > 0.0 && now - prev < dc_threshold {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::UiCenterListingAt(frame.call_site));
                            }
                            ui.ctx().memory_mut(|m| m.data.insert_temp(mem_id, 0.0_f64));
                        } else {
                            ui.ctx().memory_mut(|m| m.data.insert_temp(mem_id, now));
                        }
                    }

                    ui.add_sized([22.0, 0.0], egui::Label::new("→"));

                    let target_resp = ui.add_sized(
                        [62.0, 0.0],
                        egui::Button::new(
                            egui::RichText::new(format!("${:04X}", frame.target)).monospace(),
                        ),
                    );
                    if target_resp.clicked() {
                        eprintln!("[cs-click] target=${:04X} depth={}", frame.target, depth);
                        let mem_id = egui::Id::new(("cs_target_dc", depth));
                        let prev: f64 = ui.ctx().memory(|m| m.data.get_temp(mem_id).unwrap_or(0.0));
                        if prev > 0.0 && now - prev < dc_threshold {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::UiCenterListingAt(frame.target));
                            }
                            ui.ctx().memory_mut(|m| m.data.insert_temp(mem_id, 0.0_f64));
                        } else {
                            ui.ctx().memory_mut(|m| m.data.insert_temp(mem_id, now));
                        }
                    }

                    let ret_resp = ui.add_sized(
                        [62.0, 0.0],
                        egui::Button::new(
                            egui::RichText::new(format!("${:04X}", frame.return_addr)).monospace(),
                        ),
                    );
                    if ret_resp.clicked() {
                        eprintln!(
                            "[cs-click] return=${:04X} depth={}",
                            frame.return_addr, depth
                        );
                        let mem_id = egui::Id::new(("cs_return_dc", depth));
                        let prev: f64 = ui.ctx().memory(|m| m.data.get_temp(mem_id).unwrap_or(0.0));
                        if prev > 0.0 && now - prev < dc_threshold {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::UiCenterListingAt(frame.return_addr));
                            }
                            ui.ctx().memory_mut(|m| m.data.insert_temp(mem_id, 0.0_f64));
                        } else {
                            ui.ctx().memory_mut(|m| m.data.insert_temp(mem_id, now));
                        }
                    }
                    ui.add_sized(
                        [62.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", frame.sp_at_call)).monospace(),
                        ),
                    );
                });
            }
        });
}

pub(super) fn show_call_stack_viewport(
    ctx: &egui::Context,
    call_stack: Arc<Mutex<Vec<CallFrameInfo>>>,
    cmds: Arc<Mutex<Vec<Command>>>,
    open: bool,
) {
    let id = call_stack_viewport_id();
    if update_visible(open) {
        set_visible(ctx, open);
    }
    // See `breakpoints_ui::show_breakpoints_viewport` for the
    // `with_active(false)` rationale (macOS focus monopolisation).
    let builder = egui::ViewportBuilder::default()
        .with_title("em6809 Call Stack")
        .with_inner_size(egui::vec2(500.0, 260.0))
        .with_visible(open)
        .with_active(false)
        .with_window_level(egui::WindowLevel::Normal)
        .with_resizable(true);

    #[cfg(target_os = "windows")]
    {
        let cs_cl = Arc::clone(&call_stack);
        let cmds_cl = Arc::clone(&cmds);
        ctx.show_viewport_immediate(id, builder, move |ctx, _class| {
            if open {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_call_stack(ui, &cs_cl, &cmds_cl);
                });
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                update_visible(false);
                note_closed();
            }
        });
    }
    // See `breakpoints_ui::show_breakpoints_viewport` for the platform
    // split rationale (Linux gate-by-declaration vs macOS always-declare).
    #[cfg(target_os = "linux")]
    {
        if open {
            let cs_cl = Arc::clone(&call_stack);
            let cmds_cl = Arc::clone(&cmds);
            ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_call_stack(ui, &cs_cl, &cmds_cl);
                });
                if ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    update_visible(false);
                    note_closed();
                }
            });
        }
    }

    #[cfg(target_os = "macos")]
    {
        let cs_cl = Arc::clone(&call_stack);
        let cmds_cl = Arc::clone(&cmds);
        ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
            if open {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_call_stack(ui, &cs_cl, &cmds_cl);
                });
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                update_visible(false);
                note_closed();
            }
        });
    }
}
