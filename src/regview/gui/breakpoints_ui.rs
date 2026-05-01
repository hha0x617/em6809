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

use super::{BpInfo, Command};

static BP_VISIBLE: OnceCell<AtomicBool> = OnceCell::new();
static BP_CLOSED: OnceCell<AtomicBool> = OnceCell::new();
/// Per-BP draft of the condition expression while it's being typed.
/// Module-local because the viewport closure lives outside the
/// `RegApp` borrow scope; the `HashMap` is keyed by `Breakpoint::id`
/// so unrelated BPs don't fight over the same draft slot. Entries are
/// committed on lost-focus / Enter and removed on commit.
static BP_COND_DRAFTS: OnceCell<Mutex<HashMap<u32, String>>> = OnceCell::new();

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
                    let mut enabled = bp.enabled;
                    if ui
                        .add_sized([22.0, 0.0], egui::Checkbox::without_text(&mut enabled))
                        .changed()
                    {
                        if let Ok(mut q) = cmds.lock() {
                            q.push(Command::SetBreakpointEnabled(bp.id, enabled));
                        }
                    }
                    ui.add_sized(
                        [60.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("${:04X}", bp.addr)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [60.0, 0.0],
                        egui::Label::new(
                            egui::RichText::new(format!("{}", bp.hit_count)).monospace(),
                        ),
                    );
                    ui.add_sized(
                        [60.0, 0.0],
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
                        [120.0, 0.0],
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
                    let del = ui.add_sized([24.0, 0.0], egui::Button::new("✕"));
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
pub(super) fn show_breakpoints_viewport(
    ctx: &egui::Context,
    bp_list: Arc<Mutex<Vec<BpInfo>>>,
    cmds: Arc<Mutex<Vec<Command>>>,
    open: bool,
) {
    let id = breakpoints_viewport_id();
    if update_visible(open) {
        set_visible(ctx, open);
    }
    let builder = egui::ViewportBuilder::default()
        .with_title("em6809 Breakpoints")
        .with_inner_size(egui::vec2(440.0, 260.0))
        .with_visible(open)
        .with_window_level(egui::WindowLevel::Normal)
        .with_resizable(true);

    #[cfg(target_os = "windows")]
    {
        let bp_list_cl = Arc::clone(&bp_list);
        let cmds_cl = Arc::clone(&cmds);
        ctx.show_viewport_immediate(id, builder, move |ctx, _class| {
            if open {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_breakpoints(ui, &bp_list_cl, &cmds_cl);
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
    #[cfg(not(target_os = "windows"))]
    {
        // Only declare the deferred viewport while the window is open.
        // Dropping the declaration on the next frame after `open` flips
        // false tells egui to close the viewport. Linux/macOS deferred
        // viewports do not honor `ViewportBuilder::with_visible(false)`,
        // so without this gate the window pops up at startup despite
        // the menu showing it hidden.
        if open {
            // Pump repaints so the deferred viewport reflects parent-side
            // updates (worker-side bp_list mutations from listing-pane
            // double-clicks etc.) without requiring the user to focus
            // the child window first. Mirrors what console_ui does for
            // its own deferred viewport.
            ctx.request_repaint_of(id);
            ctx.request_repaint_after_for(std::time::Duration::from_millis(33), id);

            let bp_list_cl = Arc::clone(&bp_list);
            let cmds_cl = Arc::clone(&cmds);
            ctx.show_viewport_deferred(id, builder, move |ctx, _class| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    render_breakpoints(ui, &bp_list_cl, &cmds_cl);
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
}
