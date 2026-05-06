// Breakpoints window — a real OS-level child window (via eframe's
// viewport API), the same way the VT100 console gets its own
// top-level window. Shows the registered BP list and lets the user
// toggle / remove individual entries; mutations flow back to the
// worker as `Command::SetBreakpointEnabled` / `RemoveBreakpoint`.
//
// `show_viewport_immediate` (Windows) / `show_viewport_deferred`
// (other platforms) is what gives this its own resizable, draggable
// OS window instead of being trapped inside the eframe main viewport.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use eframe::egui;
use once_cell::sync::OnceCell;

use super::{toast_anchor_for_button, BpInfo, Command, ToastMessage, ToastOrigin};

static BP_VISIBLE: OnceCell<AtomicBool> = OnceCell::new();
static BP_CLOSED: OnceCell<AtomicBool> = OnceCell::new();
/// Per-BP draft of the condition expression while it's being typed.
/// Module-local because the viewport closure lives outside the
/// `RegApp` borrow scope; the `HashMap` is keyed by `Breakpoint::id`
/// so unrelated BPs don't fight over the same draft slot. Entries are
/// committed on lost-focus / Enter and removed on commit.
static BP_COND_DRAFTS: OnceCell<Mutex<HashMap<u32, String>>> = OnceCell::new();
/// Last `clicked()` timestamp per BP id, used to derive double-click
/// from two `clicked()` events within `multi_click_time`.  Stored
/// here (a process-local static) rather than via
/// `ctx.memory().data.insert_temp`, because that storage proved
/// unreliable across paints in our setup — on Windows we observed
/// `[bp-click]` firing twice per double-click but `prev` reading 0.0
/// each time, which would only happen if the inserted value
/// disappeared between paints.
static BP_LAST_CLICK: OnceCell<Mutex<HashMap<u32, f64>>> = OnceCell::new();

fn drafts() -> &'static Mutex<HashMap<u32, String>> {
    BP_COND_DRAFTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn breakpoints_viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("em6809_breakpoints_window")
}

fn update_visible(desired: bool) -> bool {
    let flag = BP_VISIBLE.get_or_init(|| AtomicBool::new(desired));
    let prev = flag.swap(desired, Ordering::SeqCst);
    prev != desired
}

fn set_visible(ctx: &egui::Context, visible: bool) {
    update_visible(visible);
    ctx.send_viewport_cmd_to(
        breakpoints_viewport_id(),
        egui::ViewportCommand::Visible(visible),
    );
}

fn note_closed() {
    if let Some(flag) = BP_CLOSED.get() {
        flag.store(true, Ordering::SeqCst);
    } else {
        let _ = BP_CLOSED.set(AtomicBool::new(true));
    }
}

/// Consume the "user closed the window" signal. Caller (RegApp::update)
/// reads this each frame and mirrors the result into `bp_window_open`
/// so the View menu's checkbox stays in sync with the OS X button.
pub(crate) fn take_closed() -> bool {
    BP_CLOSED
        .get()
        .map(|f| f.swap(false, Ordering::SeqCst))
        .unwrap_or(false)
}

/// Render the BP list into `ui`. Pulled out as its own function so
/// the viewport closure (which owns shared `Arc`s, not the whole app)
/// can call it without needing a `RegApp` reference.
fn render_breakpoints(
    ui: &mut egui::Ui,
    bp_list: &Arc<Mutex<Vec<BpInfo>>>,
    cmds: &Arc<Mutex<Vec<Command>>>,
) {
    let bps: Vec<BpInfo> = bp_list.lock().map(|g| g.clone()).unwrap_or_default();

    if bps.is_empty() {
        ui.weak("No breakpoints set.");
        ui.add_space(4.0);
        ui.label("Tip: double-click a row in the listing pane to add one.");
        return;
    }

    egui::ScrollArea::vertical()
        .id_salt("bp_window_scroll")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_sized([22.0, 0.0], egui::Label::new(""));
                ui.add_sized(
                    [60.0, 0.0],
                    egui::Label::new(egui::RichText::new("Addr").monospace().strong()),
                );
                ui.add_sized(
                    [60.0, 0.0],
                    egui::Label::new(egui::RichText::new("Hits").monospace().strong()),
                );
                ui.add_sized(
                    [60.0, 0.0],
                    egui::Label::new(egui::RichText::new("Skip").monospace().strong()),
                );
                ui.add_sized(
                    [120.0, 0.0],
                    egui::Label::new(egui::RichText::new("Condition").monospace().strong()),
                );
                // Delete column header — right-most for muscle memory
                // ("destructive action sits at the row's edge").
                ui.add_sized([24.0, 0.0], egui::Label::new(""));
            });
            ui.separator();

            for bp in bps.iter() {
                ui.horizontal(|ui| {
                    // Force every cell to the same height so widgets
                    // of different natural heights (Checkbox / Button
                    // / Label / TextEdit) align vertically — passing
                    // height = 0 to `add_sized` gives each cell its
                    // own natural height, which makes Buttons sit
                    // lower than Labels in the same row.
                    let row_h = ui.spacing().interact_size.y;
                    let mut enabled = bp.enabled;
                    if ui
                        .add_sized([22.0, row_h], egui::Checkbox::without_text(&mut enabled))
                        .changed()
                    {
                        if let Ok(mut q) = cmds.lock() {
                            q.push(Command::SetBreakpointEnabled(bp.id, enabled));
                        }
                    }
                    // Address cell — clickable so a double-click
                    // recentres the listing pane.  Use a default-
                    // framed `Button` (same shape as the delete-X
                    // button below, which is the only `clicked()`
                    // pattern we have empirical evidence works on
                    // every platform).  Manual timestamp logic
                    // converts two clicks within 400 ms into a
                    // double-click without relying on egui's
                    // `double_clicked()` which proved unreliable
                    // for narrow short-text widgets like this.
                    let addr_resp = ui.add_sized(
                        [60.0, row_h],
                        egui::Button::new(
                            egui::RichText::new(format!("${:04X}", bp.addr)).monospace(),
                        ),
                    );
                    if addr_resp.clicked() {
                        let now = ui.input(|i| i.time);
                        let map = BP_LAST_CLICK.get_or_init(|| Mutex::new(HashMap::new()));
                        let mut g = map.lock().unwrap();
                        let prev = g.get(&bp.id).copied().unwrap_or(0.0_f64);
                        if prev > 0.0 && now - prev < 0.4 {
                            // Anchor the eventual "not in listing"
                            // toast just below the address button —
                            // or above it when below would overflow
                            // the viewport's bottom edge — so the
                            // popup is visually tied to the click and
                            // never gets clipped.
                            let (anchor, pivot) =
                                toast_anchor_for_button(addr_resp.rect, ui.ctx().content_rect());
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::UiCenterListingAt {
                                    addr: bp.addr,
                                    origin: ToastOrigin::Breakpoints,
                                    anchor,
                                    pivot,
                                });
                            }
                            g.remove(&bp.id);
                        } else {
                            g.insert(bp.id, now);
                        }
                    }
                    ui.add_sized(
                        [60.0, row_h],
                        egui::Label::new(
                            egui::RichText::new(format!("{}", bp.hit_count)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [60.0, row_h],
                        egui::Label::new(
                            egui::RichText::new(format!("{}", bp.ignore_count)).monospace(),
                        ),
                    );
                    // Editable condition. The draft is held module-
                    // locally so it survives across paints while the
                    // user is typing. We commit on `lost_focus()`
                    // (which also fires on Enter), at which point the
                    // worker thread parses and stores the condition.
                    let stored = bp.condition.clone().unwrap_or_default();
                    let mut draft = {
                        let mut g = drafts().lock().unwrap();
                        g.entry(bp.id).or_insert_with(|| stored.clone()).clone()
                    };
                    let resp = ui.add_sized(
                        [120.0, row_h],
                        egui::TextEdit::singleline(&mut draft)
                            .font(egui::TextStyle::Monospace)
                            .hint_text("condition"),
                    );
                    if resp.changed() {
                        drafts().lock().unwrap().insert(bp.id, draft.clone());
                    }
                    if resp.lost_focus() {
                        let final_text = drafts()
                            .lock()
                            .unwrap()
                            .remove(&bp.id)
                            .unwrap_or(draft.clone());
                        if final_text != stored {
                            if let Ok(mut q) = cmds.lock() {
                                q.push(Command::SetBreakpointCondition(bp.id, final_text));
                            }
                        }
                    }
                    let del = ui.add_sized([24.0, row_h], egui::Button::new("✕"));
                    if del.clicked() {
                        if let Ok(mut q) = cmds.lock() {
                            q.push(Command::RemoveBreakpoint(bp.id));
                        }
                    }
                });
            }
        });
}

/// Mount or update the Breakpoints window as a separate OS viewport.
/// Caller passes the latest `open` value; the function reconciles it
/// with the underlying viewport's visibility, and on Windows it
/// renders inline (`show_viewport_immediate`), while on other
/// platforms it spawns a deferred viewport that survives across
/// frames (`show_viewport_deferred`).
// `CentralPanel::show(ctx, ...)` is deprecated; viewport closures only
// receive `ctx` from `show_viewport_immediate / _deferred`, so migrating
// to `show_inside(ui, ...)` requires extra Ui plumbing.  See
// `docs/en/gui_stack.md`.
#[allow(deprecated)]
pub(super) fn show_breakpoints_viewport(
    ctx: &egui::Context,
    bp_list: Arc<Mutex<Vec<BpInfo>>>,
    cmds: Arc<Mutex<Vec<Command>>>,
    toast: Arc<Mutex<Option<ToastMessage>>>,
    open: bool,
) {
    let id = breakpoints_viewport_id();
    if update_visible(open) {
        set_visible(ctx, open);
    }
    // `with_active(false)` is the macOS-relevant bit: tell AppKit not
    // to promote this NSWindow to the key window when it appears, so
    // typing into the main window or VT100 console keeps working
    // while the BP / Call Stack panels are open.  Documented as
    // unsupported on X11 / Wayland in egui, but harmless there — the
    // flag is simply ignored.
    let builder = egui::ViewportBuilder::default()
        .with_title("em6809 Breakpoints")
        .with_inner_size(egui::vec2(440.0, 260.0))
        .with_visible(open)
        .with_active(false)
        .with_window_level(egui::WindowLevel::Normal)
        .with_resizable(true);

    #[cfg(target_os = "windows")]
    {
        let bp_list_cl = Arc::clone(&bp_list);
        let cmds_cl = Arc::clone(&cmds);
        let toast_cl = Arc::clone(&toast);
        ctx.show_viewport_immediate(id, builder, move |ctx, _class| {
            if open {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_breakpoints(ui, &bp_list_cl, &cmds_cl);
                });
                super::render_toast_for(ctx, &toast_cl, ToastOrigin::Breakpoints);
            }
            if ctx.input(|i| i.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                update_visible(false);
                note_closed();
            }
        });
    }
    // Linux: deferred viewports do not honor
    // `ViewportBuilder::with_visible(false)`, so we have to gate the
    // declaration itself — dropping it on the next frame closes the
    // viewport.  See git log for the original WSLg-startup bug.
    #[cfg(target_os = "linux")]
    {
        if open {
            let bp_list_cl = Arc::clone(&bp_list);
            let cmds_cl = Arc::clone(&cmds);
            let toast_cl = Arc::clone(&toast);
            ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_breakpoints(ui, &bp_list_cl, &cmds_cl);
                });
                super::render_toast_for(ctx, &toast_cl, ToastOrigin::Breakpoints);
                if ctx.input(|i| i.viewport().close_requested()) {
                    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                    update_visible(false);
                    note_closed();
                }
            });
        }
    }

    // macOS: declare the viewport unconditionally and toggle
    // visibility with `Visible(bool)` viewport commands instead.
    // Re-declaring after an `if open == false` gap creates a fresh
    // NSWindow each time the user re-opens the panel, and AppKit
    // promotes the new NSWindow to the key window — that's what
    // monopolised input from the main window and the VT100 console
    // on macOS.  Mirrors what `console_ui` does for its own
    // deferred viewport.  Render is still guarded by `open` so a
    // hidden viewport doesn't spend time laying out the table.
    #[cfg(target_os = "macos")]
    {
        let bp_list_cl = Arc::clone(&bp_list);
        let cmds_cl = Arc::clone(&cmds);
        let toast_cl = Arc::clone(&toast);
        ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
            if open {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_breakpoints(ui, &bp_list_cl, &cmds_cl);
                });
                super::render_toast_for(ctx, &toast_cl, ToastOrigin::Breakpoints);
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
