use super::*;
impl User {
    pub(super) fn default_action<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<Action> {
        if args[1] != 0x47 {
            return self.default_proc(process, args).map(Action::Return);
        }
        let fields = words::<_, 7>(process, args[3])?;
        let Some(window) = self.windows.get(&args[0]) else {
            return Ok(Action::Return(self.fail(1400)));
        };
        let mut messages = Vec::new();
        if fields[6] & 2 == 0 {
            messages.push((3, 0, (window.x & 0xffff) | ((window.y & 0xffff) << 16)));
        }
        if fields[6] & 1 == 0 {
            messages.push((
                5,
                0,
                (window.width & 0xffff) | ((window.height & 0xffff) << 16),
            ));
        }
        self.notify(args[0], messages)
    }
    pub(super) fn notify(
        &mut self,
        hwnd: u32,
        mut messages: Vec<(u32, u32, u32)>,
    ) -> Result<Action> {
        if messages.is_empty() {
            return Ok(Action::Return(0));
        }
        let (message, wparam, lparam) = messages.remove(0);
        self.invoke(
            hwnd,
            message,
            wparam,
            lparam,
            Continuation::Notifications { hwnd, messages },
        )
    }

    /// The virtual desktop has no nonclient area. Reject requests requiring a
    /// frame/menu instead of claiming dimensions inconsistent with our windows.
    pub(super) fn adjust_rect<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
        extended: bool,
    ) -> Result<u32> {
        let rect = words::<_, 4>(process, args[0])?;
        let style = args[1];
        // Caption (border/dialog frame), resize frame, scrollbars and system menu.
        let decorations = 0x00fc_0000;
        let ex_style = if extended { args[3] } else { 0 };
        // These flags do not introduce a nonclient area on a borderless window:
        // TOPMOST, TRANSPARENT, TOOLWINDOW, APPWINDOW and NOACTIVATE.
        let inert_extended = 0x0804_00a8;
        if args[2] != 0 || style & decorations != 0 || ex_style & !inert_extended != 0 {
            return Ok(self.fail(120)); // ERROR_CALL_NOT_IMPLEMENTED
        }
        // Validate writable output even though the borderless result is unchanged.
        write_words(process, args[0], &rect)?;
        Ok(1)
    }

    pub(super) fn rect<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        hwnd: u32,
        address: u32,
        client: bool,
    ) -> Result<u32> {
        let Some(w) = self.windows.get(&hwnd) else {
            return Ok(self.fail(1400));
        };
        let (x, y) = if client { (0, 0) } else { (w.x, w.y) };
        write_words(
            process,
            address,
            &[x, y, x.wrapping_add(w.width), y.wrapping_add(w.height)],
        )?;
        Ok(1)
    }
    pub(super) fn coordinate<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        hwnd: u32,
        address: u32,
        to_screen: bool,
    ) -> Result<u32> {
        let Some(w) = self.windows.get(&hwnd) else {
            return Ok(self.fail(1400));
        };
        let [x, y] = words(process, address)?;
        let point = if to_screen {
            [x.wrapping_add(w.x), y.wrapping_add(w.y)]
        } else {
            [x.wrapping_sub(w.x), y.wrapping_sub(w.y)]
        };
        write_words(process, address, &point)?;
        Ok(1)
    }
    pub(super) fn default_proc<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        match a[1] {
            0x81 => Ok(1), // WM_NCCREATE must succeed by default.
            0x84 => Ok(1), // WM_NCHITTEST: the canvas is entirely client area.
            0x24 => {
                // WM_GETMINMAXINFO
                write_words(
                    process,
                    a[3],
                    &[
                        0,
                        0,
                        self.display.mode().0,
                        self.display.mode().1,
                        0,
                        0,
                        1,
                        1,
                        self.display.mode().0,
                        self.display.mode().1,
                    ],
                )?;
                Ok(0)
            }
            _ => Ok(0),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/user/geometry.rs"]
mod tests;
