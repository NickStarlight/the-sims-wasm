use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct Message {
    pub hwnd: u32,
    pub id: u32,
    pub wparam: u32,
    pub lparam: u32,
    pub time: u32,
    pub x: u32,
    pub y: u32,
    pub input: bool,
}
impl Message {
    pub(super) fn new(hwnd: u32, id: u32, wparam: u32, lparam: u32) -> Self {
        Self {
            hwnd,
            id,
            wparam,
            lparam,
            time: 0,
            x: 0,
            y: 0,
            input: false,
        }
    }
    pub(super) fn read<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<Self> {
        let [hwnd, id, wparam, lparam, time, x, y] = words(process, address)?;
        Ok(Self {
            hwnd,
            id,
            wparam,
            lparam,
            time,
            x,
            y,
            input: false,
        })
    }
    fn write<M: GuestMemory>(&self, process: &mut Process<M>, address: u32) -> Result<()> {
        write_words(
            process,
            address,
            &[
                self.hwnd,
                self.id,
                self.wparam,
                self.lparam,
                self.time,
                self.x,
                self.y,
            ],
        )
    }
}
impl User {
    pub(crate) fn post_notification(&mut self, hwnd: u32, id: u32, data: u32) -> Result<()> {
        self.post(Message::new(hwnd, id, 0, data))?;
        Ok(())
    }
    pub(super) fn enqueue(&mut self, message: Message) -> Result<()> {
        if self.messages.len() == MAX_MESSAGES {
            return Err(Error::Host("window message queue capacity exceeded".into()));
        }
        self.messages.push_back(message);
        Ok(())
    }
    pub(super) fn post(&mut self, message: Message) -> Result<u32> {
        if message.hwnd == 0xffff {
            let windows: Vec<_> = self.windows.keys().copied().collect();
            if self.messages.len() + windows.len() > MAX_MESSAGES {
                return Ok(self.fail(1816));
            }
            for hwnd in windows {
                self.enqueue(Message { hwnd, ..message })?;
            }
        } else {
            if message.hwnd != 0 && !self.windows.contains_key(&message.hwnd) {
                return Ok(self.fail(1400));
            }
            self.enqueue(message)?;
        }
        Ok(1)
    }
    pub(super) fn peek<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        if a[4] & !3 != 0 {
            return Err(Error::Host(
                "PeekMessageA queue-status flags are not implemented".into(),
            ));
        }
        if a[1] != 0 && a[1] != u32::MAX && !self.windows.contains_key(&a[1]) {
            return Ok(self.fail(1400));
        }
        self.refill()?;
        let index = self.messages.iter().position(|message| {
            let m = Message {
                hwnd: self.destination(message),
                ..*message
            };
            m.id == 0x12
                || ((a[1] == 0
                    || (a[1] == u32::MAX && m.hwnd == 0)
                    || m.hwnd == a[1]
                    || m.hwnd == 0)
                    && ((a[2] == 0 && a[3] == 0) || (a[2]..=a[3]).contains(&m.id)))
        });
        let Some(index) = index else {
            return Ok(0);
        };
        let message = Message {
            hwnd: self.destination(&self.messages[index]),
            ..self.messages[index]
        };
        message.write(process, a[0])?;
        if a[4] & 1 != 0 {
            if let Some(message) = self.messages.remove(index) {
                self.consume_input(message);
            }
        }
        Ok(1)
    }
}
