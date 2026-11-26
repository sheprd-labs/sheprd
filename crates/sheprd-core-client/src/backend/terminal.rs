use crate::{Attachment, Core, End, Error, Result, wire::Attach};
use sheprd_backend::{
    ClientPaneInputEvent, Exit, History, HistoryAnswer, Role, Size, SurfaceUpdate, TerminalId,
};
use sheprd_terminal::{Emulator, Surface};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How often an observer asks the core for its terminal's size, which the
/// active viewer sets and nothing on an attachment reports.
const SIZE_CHECK: Duration = Duration::from_millis(500);

/// An attached herdr-core terminal, emulated here: its raw output becomes
/// cells, and semantic input becomes bytes for its program's modes.
pub struct CoreTerminal {
    attachment: Attachment,
    emulator: Emulator,
    end: Option<End>,
    surface: Surface,
    role: Role,
    /// An observer's core and when it last read the terminal's size there;
    /// an active view sets the size itself.
    sizing: Option<(Arc<Core>, Instant)>,
}

impl CoreTerminal {
    pub(super) fn attach(
        core: &Arc<Core>,
        id: &TerminalId,
        size: Size,
        role: Role,
        scrollback: usize,
    ) -> Result<Self> {
        // The core keeps each terminal's recent history and paints what it
        // has of it first, so a view attached anew (the app restarted, the
        // core reconnected) starts with the output it missed.
        let attachment = core.attach(&Attach {
            cols: size.cols(),
            history: u32::try_from(scrollback).unwrap_or(u32::MAX),
            observe: role == Role::Observer,
            rows: size.rows(),
            terminal: id.as_str(),
            ..Attach::default()
        })?;
        // An observer sees the screen at the size its active viewer set.
        let emulated = match role {
            Role::Active => size,
            Role::Observer => {
                Size::new(attachment.terminal.cols, attachment.terminal.rows).unwrap_or(size)
            }
        };
        Ok(Self {
            attachment,
            emulator: Emulator::new(emulated, scrollback),
            end: None,
            surface: Surface::default(),
            role,
            sizing: (role == Role::Observer).then(|| (Arc::clone(core), Instant::now())),
        })
    }

    /// Keeps an observer's screen at the size the active viewer set, read
    /// from the core now and then. A failed read leaves the size as it is;
    /// a lost connection ends the attachment too, which reports it.
    fn follow_size(&mut self) {
        let Some((core, checked)) = &mut self.sizing else {
            return;
        };
        if checked.elapsed() < SIZE_CHECK {
            return;
        }
        *checked = Instant::now();
        let address = &self.attachment.terminal.address;
        let size = match core.list(false) {
            Ok(list) => list
                .terminals
                .iter()
                .find(|terminal| terminal.address == *address)
                .and_then(|terminal| Size::new(terminal.cols, terminal.rows).ok()),
            Err(error) => {
                tracing::debug!(%error, "herdr-core observer size unread");
                None
            }
        };
        if let Some(size) = size
            && size != self.emulator.size()
        {
            self.emulator.resize(size);
        }
    }

    fn drives(&self) -> bool {
        self.role == Role::Active
    }
}

impl sheprd_backend::Terminal for CoreTerminal {
    type Error = Error;

    /// A wheel turn the program does not read scrolls this view's history,
    /// an observer's too. Typing returns the view to the live screen.
    fn input(&mut self, input: &ClientPaneInputEvent) -> Result<()> {
        let bytes = self.emulator.input_bytes(input, self.role);
        if bytes.is_empty() {
            return Ok(());
        }
        self.attachment.write(&bytes)
    }

    /// An active view sizes the program's terminal. An observer keeps the
    /// size the program draws at, whatever its own view's bounds, so its
    /// output wraps where the program's does.
    fn resize(&mut self, size: Size) -> Result<()> {
        if !self.drives() {
            return Ok(());
        }
        self.emulator.resize(size);
        self.attachment.resize(size.cols(), size.rows())
    }

    /// The emulator sizes images by it; an active view tells the core too,
    /// which answers the program's questions about it.
    fn cell_pixels(&mut self, width: u16, height: u16) -> Result<()> {
        self.emulator
            .set_cell_pixels(u32::from(width), u32::from(height));
        if !self.drives() {
            return Ok(());
        }
        self.attachment.facts(&crate::wire::Display {
            cell: Some([u32::from(width), u32::from(height)]),
            ..crate::wire::Display::default()
        })
    }

    /// The core reports focus to programs that asked for it.
    fn focus(&mut self, focused: bool) -> Result<()> {
        if !self.drives() {
            return Ok(());
        }
        self.attachment.focus(focused)
    }

    fn next(&mut self, timeout: Duration) -> Result<Vec<SurfaceUpdate>> {
        match &self.end {
            Some(End::Lost) => return Err(Error::Lost),
            // The program is gone but its history stays: what a history
            // request changed (a scroll, a clear) is still shown.
            Some(End::Closed(_)) => {
                let mut updates = Vec::new();
                // No byte can end a held update now: show what it held.
                self.emulator.end_sync();
                self.surface.push_view(&mut self.emulator, &mut updates);
                return Ok(updates);
            }
            None => {}
        }
        // A held update is released on time even when no bytes follow it.
        let timeout = self.surface.timeout(timeout);
        self.follow_size();
        let chunk = self.attachment.read(timeout)?;
        self.emulator.feed(&chunk.bytes);
        let mut updates = self
            .surface
            .updates(&mut self.emulator, chunk.end.is_some());
        match chunk.end {
            Some(End::Closed(closed)) => {
                updates.push(SurfaceUpdate::Ended(Exit {
                    code: closed.exit_code,
                }));
                self.end = Some(End::Closed(closed));
            }
            // What arrived before the loss is still shown; the next call fails.
            Some(End::Lost) if updates.is_empty() => return Err(Error::Lost),
            Some(End::Lost) => self.end = Some(End::Lost),
            None => {}
        }
        Ok(updates)
    }

    /// Answered from this view's emulator, an observer's too: scrolling
    /// and clearing change what this view shows, never the program.
    fn history(&mut self, request: &History) -> Result<HistoryAnswer> {
        Ok(self.emulator.history(request)?)
    }
}
