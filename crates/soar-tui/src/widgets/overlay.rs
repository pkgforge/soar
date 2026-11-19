//! The dialogs that draw over the active view and capture every key press
//! until the user closes or accepts them.

use ratatui::{
    Frame,
    crossterm::event::{KeyCode, KeyEvent},
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Clear, Padding, Paragraph},
};
use soar_core::database::models::InstalledPackage;
use soar_operations::privilege::EscalationTool;

use crate::{
    app::App,
    privileged::Secret,
    theme::Theme,
    widgets::{
        authorize::Authorize,
        confirm::Confirm,
        form::{Form, FormValues},
        help::Help,
        picker::Picker,
        text_input::Prompt,
    },
};
/// A dialog drawn over the active view. While one is open it captures every
/// key press.
#[derive(Debug)]
pub(crate) enum Overlay {
    /// A yes-or-no dialog that names everything about to change.
    Confirm(Confirm),
    /// A list of rows, accepted by index.
    Picker(Picker),
    /// A single line of text, accepted as that text.
    Prompt(Prompt),
    /// Checkboxes and text fields, accepted as their values.
    Form(Form),
    /// The key reference of the active view.
    Help(Help),
    /// The choice of how to quit while a job runs.
    QuitGuard(QuitGuard),
    /// The administrator password dialog for the system pass.
    Authorize(Authorize),
}

impl Overlay {
    /// Hands one pressed key to the overlay and reports what it decided.
    pub(crate) fn handle(self, key: KeyEvent) -> OverlayOutcome {
        match self {
            Overlay::Confirm(confirm) => confirm.handle(key),
            Overlay::Picker(picker) => picker.handle(key),
            Overlay::Prompt(prompt) => prompt.handle(key),
            Overlay::Form(form) => form.handle(key),
            Overlay::Help(help) => help.handle(key),
            Overlay::QuitGuard(guard) => guard.handle(key),
            Overlay::Authorize(dialog) => dialog.handle(key),
        }
    }

    /// Draws the overlay centered in `area`, over what it holds, and
    /// returns the rect it drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        match self {
            Overlay::Confirm(confirm) => confirm.render(f, area, app),
            Overlay::Picker(picker) => picker.render(f, area, app),
            Overlay::Prompt(prompt) => prompt.render(f, area, app),
            Overlay::Form(form) => form.render(f, area, app),
            Overlay::Help(help) => help.render(f, area, app),
            Overlay::QuitGuard(guard) => guard.render(f, area, app),
            Overlay::Authorize(dialog) => dialog.render(f, area, app),
        }
    }
}

/// The dialog that opens when quitting while a job runs: wait for it, stop
/// it and quit, or stay. The focus starts on cancel so an accidental enter
/// cannot act.
#[derive(Debug)]
pub(crate) struct QuitGuard {
    /// What runs and what waits behind it.
    body: Vec<Line<'static>>,
    /// The focused button: 0 cancel, 1 wait, 2 abort and quit.
    focus: usize,
}

/// The guard's buttons in focus order.
const QUIT_BUTTONS: [&str; 3] = ["Cancel", "Wait and quit", "Abort and quit"];

impl QuitGuard {
    /// Builds the guard naming the running job and how much is queued
    /// behind it.
    pub(crate) fn new(running_title: &str, queued: usize) -> QuitGuard {
        let mut body = vec![Line::raw(format!("A job is running: {running_title}."))];
        if queued > 0 {
            body.push(Line::raw(format!("{queued} more queued.")));
        }
        QuitGuard {
            body,
            focus: 0,
        }
    }

    /// Acts on one pressed key. The movement keys cycle the focus, enter
    /// presses the focused button, and esc and n stay.
    pub(crate) fn handle(mut self, key: KeyEvent) -> OverlayOutcome {
        match (key.code, key.modifiers) {
            (
                KeyCode::Left
                | KeyCode::Right
                | KeyCode::Tab
                | KeyCode::Char('h')
                | KeyCode::Char('l'),
                _,
            ) => {
                self.focus = (self.focus + 1) % QUIT_BUTTONS.len();
                OverlayOutcome::Stay(Overlay::QuitGuard(self))
            }
            (KeyCode::Enter, _) => {
                match self.focus {
                    0 => OverlayOutcome::Close,
                    1 => OverlayOutcome::Accept(OnAccept::QuitWait, None),
                    _ => OverlayOutcome::Accept(OnAccept::QuitAbort, None),
                }
            }
            (KeyCode::Esc, _) | (KeyCode::Char('n'), _) => OverlayOutcome::Close,
            _ => OverlayOutcome::Stay(Overlay::QuitGuard(self)),
        }
    }

    /// Draws the guard centered in the body area, and returns the rect it
    /// drew into.
    pub(crate) fn render(&self, f: &mut Frame, area: Rect, app: &App) -> Rect {
        let theme = &app.theme;
        let width = area.width.saturating_sub(4).min(64);
        let rect = centered(area, width, self.body.len() as u16 + 6);
        f.render_widget(Clear, rect);

        let mut lines = vec![Line::raw("")];
        lines.extend(self.body.iter().cloned());
        lines.push(Line::raw(""));
        let mut spans = Vec::new();
        for (i, label) in QUIT_BUTTONS.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            let style = if i == self.focus {
                theme.cursor_row()
            } else {
                theme.text()
            };
            spans.push(Span::styled(format!("[ {label} ]"), style));
        }
        lines.push(Line::from(spans).right_aligned());

        let block = frame(Line::styled(" Quit ", theme.title()), None, false, theme);
        f.render_widget(Paragraph::new(lines).block(block), rect);
        rect
    }
}

/// What an accepted overlay asks the interface to do.
#[derive(Debug)]
pub(crate) enum OnAccept {
    /// Wait for the running job to finish, then quit.
    QuitWait,
    /// Stop the running job, then quit once it stops.
    QuitAbort,
    /// Quit right away, leaving the running job where it is.
    QuitForce,
    /// Stops the running job, after the abort confirm was accepted.
    Abort,
    /// Queues the removal of the installed packages the confirm listed,
    /// exactly the rows the user chose.
    Remove(Vec<InstalledPackage>),
    /// Queues the updates the confirm listed: the selected entries move
    /// out of the ready list when the accept lands.
    UpdateNow,
    /// Records the variant the picker chose for an ambiguous install
    /// query, then moves on to the next step of the flow.
    PickVariant,
    /// Queues the install the confirm listed.
    InstallNow,
    /// Builds the install choices out of the accepted options form, then
    /// resolves the selected rows with them.
    InstallWithOptions,
    /// Starts the install flow for the version the picker chose.
    PickVersion,
    /// Starts the install flow for the queries the prompt collected.
    InstallByQuery,
    /// Prepares the run of the row the prompt was opened for, with the
    /// arguments the prompt collected.
    RunArguments,
    /// Prepares the run again for the exact variant the picker chose.
    RunVariant,
    /// Links the variant the variants picker chose, keeping nothing when
    /// the active one was chosen.
    SwitchVariant,
    /// Links the named variant the switch confirm accepted.
    SwitchConfirm {
        /// The database id of the variant to link.
        id: u64,
        /// The package name, for the job's title.
        name: String,
    },
    /// Enables or disables the repository the confirm named.
    RepoEnabled {
        /// The repository to change.
        name: String,
        /// Whether it becomes enabled.
        enabled: bool,
    },
    /// Removes the repository the confirm named.
    RepoRemoved {
        /// The repository to remove.
        name: String,
    },
    /// Queues the cache clean the health confirm named.
    CleanCacheConfirm,
    /// Queues the removal of the broken symlinks the health confirm
    /// listed.
    FixSymlinksConfirm,
    /// Queues the removal of the broken packages the health confirm
    /// listed.
    RemoveBrokenConfirm,
    /// Adds the repository the form collected, then queues a sync so its
    /// packages appear in Browse.
    AddRepo,
    /// Queues the apply of the declarative packages file, moving the
    /// loaded plan's diff into the job.
    ApplyPlan,
    /// Runs the system pass with the terminal handed over, so doas can
    /// ask for the password itself. The tool runs the pass; everything
    /// else about the job is read from the queue.
    SystemTerminal {
        /// The escalation tool that asks for the password.
        tool: EscalationTool,
    },
    /// Stands in for a real action, letting tests observe an accepted
    /// overlay.
    #[cfg(test)]
    Test,
}

/// The answer of an accepted overlay.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Value {
    /// The index of the chosen picker row.
    Index(usize),
    /// The text the prompt collected, already trimmed.
    Text(String),
    /// The values of every form field, in field order.
    Form(FormValues),
}

/// What an overlay decided to do with one pressed key.
#[derive(Debug)]
pub(crate) enum OverlayOutcome {
    /// The overlay stays open and keeps the state it built. It carries
    /// itself back because the handler took ownership to be able to hand
    /// over its accept decision.
    Stay(Overlay),
    /// The overlay closed without acting.
    Close,
    /// The user accepted: the overlay carries what to do and its answer.
    Accept(OnAccept, Option<Value>),
    /// The password dialog submitted: the secret goes straight to the
    /// runner's password check, never through the debug-derived accept
    /// payloads, and the interface waits for the check's answer.
    Authenticate(Secret),
}

/// The rect of a dialog `width` by `height` centered in `area`, never
/// leaving it.
pub(crate) fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x.saturating_add(area.width.saturating_sub(width) / 2),
        y: area
            .y
            .saturating_add(area.height.saturating_sub(height) / 2),
        width,
        height,
    }
}

/// The shared dialog chrome: a rounded border in the danger or accent
/// color, one column of padding, and the footer right-aligned on the bottom
/// border when one is given.
pub(crate) fn frame<'a>(
    title: Line<'a>,
    footer: Option<Line<'a>>,
    danger: bool,
    theme: &Theme,
) -> Block<'a> {
    let border = if danger {
        theme.danger()
    } else {
        theme.accent()
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(title)
        .padding(Padding::horizontal(1));
    match footer {
        Some(footer) => block.title_bottom(footer.right_aligned()),
        None => block,
    }
}

/// The bottom row of a dialog: the cancel and action buttons with the
/// focused one highlighted. In a danger dialog the action text keeps the
/// danger style while unfocused.
pub(crate) fn buttons(
    action_label: &str,
    cancel_focused: bool,
    action_focused: bool,
    danger: bool,
    theme: &Theme,
) -> Line<'static> {
    let focused = theme.cursor_row();
    let plain = theme.text();
    let cancel = Span::styled("[ Cancel ]", if cancel_focused { focused } else { plain });
    let action_style = if action_focused {
        focused
    } else if danger {
        theme.danger()
    } else {
        plain
    };
    let action = Span::styled(format!("[ {action_label} ]"), action_style);
    Line::from(vec![cancel, Span::raw("  "), action])
}

#[cfg(test)]
mod tests {
    use super::centered;

    #[test]
    fn a_dialog_bigger_than_the_area_fills_it() {
        let area = ratatui::layout::Rect::new(0, 0, 40, 10);
        assert_eq!(centered(area, 100, 20), area);
    }

    #[test]
    fn a_small_dialog_sits_in_the_middle() {
        let area = ratatui::layout::Rect::new(5, 5, 40, 10);
        let rect = centered(area, 10, 4);
        assert_eq!(rect.x, 20);
        assert_eq!(rect.y, 8);
        assert_eq!(rect.width, 10);
        assert_eq!(rect.height, 4);
    }
}
