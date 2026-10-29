//! Ordered owned event waits ported from runtime-rs/src/sync/events.rs.
use super::*;
use serde::Serialize;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy)]
enum Object {
    Event {
        manual_reset: bool,
        signaled: bool,
    },
    Mutex {
        owner: Option<u32>,
        depth: u32,
        abandoned: bool,
    },
}
type Handle = Rc<RefCell<Object>>;
impl Object {
    fn available(&self, thread: u32) -> bool {
        match self {
            Self::Event { signaled, .. } => *signaled,
            Self::Mutex { owner, depth, .. } => {
                (owner.is_none() || *owner == Some(thread)) && *depth < u32::MAX
            }
        }
    }
    fn acquire(&mut self, thread: u32) -> bool {
        match self {
            Self::Event {
                manual_reset,
                signaled,
            } => {
                if !*manual_reset {
                    *signaled = false;
                }
                false
            }
            Self::Mutex {
                owner,
                depth,
                abandoned,
            } => {
                *owner = Some(thread);
                *depth += 1;
                std::mem::take(abandoned)
            }
        }
    }
}
struct Wait {
    thread: u32,
    objects: Vec<u32>,
    all: bool,
    deadline: Option<f64>,
    result: Option<u32>,
    sleep: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadyWait {
    pub token: u32,
    pub thread_id: u32,
    pub value: u32,
}
pub(super) struct Events {
    handles: BTreeMap<u32, Handle>,
    names: BTreeMap<String, Handle>,
    closed: BTreeSet<u32>,
    next: u32,
    waits: BTreeMap<u32, Wait>,
    next_wait: u32,
}
impl Default for Events {
    fn default() -> Self {
        Self {
            handles: BTreeMap::new(),
            names: BTreeMap::new(),
            closed: BTreeSet::new(),
            next: 0x60000000,
            waits: BTreeMap::new(),
            next_wait: 1,
        }
    }
}
impl Events {
    pub(super) fn create(&mut self, manual: bool, signaled: bool) -> Result<u32> {
        self.insert(Rc::new(RefCell::new(Object::Event {
            manual_reset: manual,
            signaled,
        })))
    }
    fn insert(&mut self, object: Handle) -> Result<u32> {
        if self.handles.len() >= 65536 {
            return Err(Error::OutOfMemory);
        }
        let id = self.next;
        self.next += 1;
        self.handles.insert(id, object);
        Ok(id)
    }
    fn named(&mut self, name: Option<String>, object: Object) -> Result<(u32, bool)> {
        if let Some(existing) = name.as_ref().and_then(|name| self.names.get(name)) {
            if std::mem::discriminant(&*existing.borrow()) != std::mem::discriminant(&object) {
                return Err(Error::InvalidAllocation);
            }
            let existing = existing.clone();
            return Ok((self.insert(existing)?, true));
        }
        let object = Rc::new(RefCell::new(object));
        let handle = self.insert(object.clone())?;
        if let Some(name) = name {
            self.names.insert(name, object);
        }
        Ok((handle, false))
    }
    pub(super) fn live(&self, id: u32) -> bool {
        self.handles.contains_key(&id) && !self.closed.contains(&id)
    }
    pub(super) fn close(&mut self, id: u32) -> bool {
        if !self.live(id) {
            false
        } else {
            self.closed.insert(id);
            let object = &self.handles[&id];
            if !self.handles.iter().any(|(handle, candidate)| {
                !self.closed.contains(handle) && Rc::ptr_eq(candidate, object)
            }) {
                self.names
                    .retain(|_, candidate| !Rc::ptr_eq(candidate, object));
            }
            true
        }
    }
    pub(super) fn signal(&mut self, id: u32) {
        if let Some(event) = self.handles.get(&id) {
            if let Object::Event { signaled, .. } = &mut *event.borrow_mut() {
                *signaled = true;
            }
        }
        self.wake();
    }
    fn wake(&mut self) {
        for wait in self.waits.values_mut() {
            if wait.result.is_none() && !wait.objects.is_empty() {
                wait.result = acquire(&self.handles, &wait.objects, wait.all, wait.thread);
            }
        }
    }
    pub(super) fn abandon(&mut self, thread: u32) {
        for object in self.handles.values() {
            if let Object::Mutex {
                owner,
                depth,
                abandoned,
            } = &mut *object.borrow_mut()
            {
                if *owner == Some(thread) {
                    *owner = None;
                    *depth = 0;
                    *abandoned = true;
                }
            }
        }
        self.wake();
    }
    fn release(&mut self, id: u32, thread: u32) -> std::result::Result<(), u32> {
        {
            let mut object = self.handles[&id].borrow_mut();
            match &mut *object {
                Object::Mutex { owner, depth, .. } if *owner == Some(thread) => {
                    *depth -= 1;
                    if *depth == 0 {
                        *owner = None;
                    }
                }
                Object::Mutex { .. } => return Err(288),
                _ => return Err(6),
            }
        }
        self.wake();
        Ok(())
    }
    pub(super) fn begin(
        &mut self,
        thread: u32,
        objects: Vec<u32>,
        all: bool,
        timeout: u32,
        now: f64,
        sleep: bool,
    ) -> Result<super::scheduling::ScheduleOutcome> {
        use super::scheduling::ScheduleOutcome;
        if !sleep
            && (objects.is_empty()
                || objects.len() > 64
                || objects
                    .iter()
                    .enumerate()
                    .any(|(i, h)| !self.live(*h) || objects[..i].contains(h)))
        {
            return Err(Error::InvalidAllocation);
        }
        if !sleep {
            if let Some(value) = acquire(&self.handles, &objects, all, thread) {
                return Ok(ScheduleOutcome::Return { value });
            }
            if timeout == 0 {
                return Ok(ScheduleOutcome::Return { value: 258 });
            }
        }
        let deadline = if timeout == u32::MAX {
            None
        } else {
            Some(now + f64::from(timeout))
        };
        let token = self.pending(thread, objects, all, deadline, sleep)?;
        Ok(ScheduleOutcome::Wait {
            token,
            deadline_ms: deadline,
        })
    }
    fn pending(
        &mut self,
        thread: u32,
        objects: Vec<u32>,
        all: bool,
        deadline: Option<f64>,
        sleep: bool,
    ) -> Result<u32> {
        if self.waits.len() >= 4096 {
            return Err(Error::OutOfMemory);
        }
        let token = self.next_wait;
        self.next_wait = self.next_wait.checked_add(1).ok_or(Error::OutOfMemory)?;
        self.waits.insert(
            token,
            Wait {
                thread,
                objects,
                all,
                deadline,
                result: None,
                sleep,
            },
        );
        Ok(token)
    }
    pub(super) fn lock_wait(&mut self, thread: u32) -> Result<u32> {
        self.pending(thread, Vec::new(), false, None, false)
    }
    pub(super) fn complete(&mut self, token: u32, value: u32) {
        if let Some(wait) = self.waits.get_mut(&token) {
            if wait.result.is_none() {
                wait.result = Some(value);
            }
        }
    }
    pub(super) fn advance(&mut self, now: f64) {
        for wait in self.waits.values_mut() {
            if wait.result.is_none() && wait.deadline.is_some_and(|deadline| now >= deadline) {
                wait.result = Some(if wait.sleep { 0 } else { 258 });
            }
        }
    }
    pub(super) fn has_ready(&self) -> bool {
        self.waits.values().any(|wait| wait.result.is_some())
    }
    pub(super) fn take_ready(&mut self) -> Vec<ReadyWait> {
        let tokens: Vec<_> = self
            .waits
            .iter()
            .filter(|(_, w)| w.result.is_some())
            .map(|(&t, _)| t)
            .collect();
        tokens
            .into_iter()
            .map(|token| {
                let w = self.waits.remove(&token).unwrap();
                ReadyWait {
                    token,
                    thread_id: w.thread,
                    value: w.result.unwrap(),
                }
            })
            .collect()
    }
    pub(super) fn cancel(&mut self, token: u32) -> bool {
        self.waits.remove(&token).is_some()
    }
}
fn acquire(
    objects: &BTreeMap<u32, Handle>,
    handles: &[u32],
    all: bool,
    thread: u32,
) -> Option<u32> {
    let available = |handle: &u32| {
        objects
            .get(handle)
            .is_some_and(|object| object.borrow().available(thread))
    };
    if all {
        if !handles.iter().all(available) {
            return None;
        }
        let mut abandoned = false;
        for handle in handles {
            abandoned |= objects[handle].borrow_mut().acquire(thread);
        }
        Some(if abandoned { 0x80 } else { 0 })
    } else {
        let index = handles.iter().position(available)?;
        let abandoned = objects[&handles[index]].borrow_mut().acquire(thread);
        Some(index as u32 + if abandoned { 0x80 } else { 0 })
    }
}
impl Kernel {
    pub(super) fn create_named_object<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let name_address = args[if call == Call::CreateEventA { 3 } else { 2 }];
        let name = if name_address == 0 {
            None
        } else {
            let bytes = process.read_c_string(name_address, 261)?;
            Some(bytes.into_iter().map(char::from).collect())
        };
        let object = if call == Call::CreateEventA {
            Object::Event {
                manual_reset: args[1] != 0,
                signaled: args[2] != 0,
            }
        } else {
            Object::Mutex {
                owner: (args[1] != 0).then_some(self.thread_id),
                depth: u32::from(args[1] != 0),
                abandoned: false,
            }
        };
        match self.events.named(name, object) {
            Ok((handle, exists)) => {
                self.thread().error = if exists { 183 } else { 0 };
                Ok(handle)
            }
            Err(Error::InvalidAllocation) => Ok(self.fail(6)),
            result => result.map(|(handle, _)| handle),
        }
    }
    pub(super) fn event_call(&mut self, call: Call, args: &[u32]) -> Result<u32> {
        if !self.events.live(args[0]) {
            self.fail(6);
            return Ok(if call == Call::WaitForSingleObject {
                u32::MAX
            } else {
                0
            });
        }
        match call {
            Call::ReleaseMutex => match self.events.release(args[0], self.thread_id) {
                Ok(()) => Ok(1),
                Err(error) => Ok(self.fail(error)),
            },
            Call::SetEvent | Call::PulseEvent | Call::ResetEvent
                if !matches!(
                    *self.events.handles[&args[0]].borrow(),
                    Object::Event { .. }
                ) =>
            {
                Ok(self.fail(6))
            }
            Call::SetEvent | Call::PulseEvent => {
                self.events.signal(args[0]);
                if call == Call::PulseEvent {
                    if let Object::Event { signaled, .. } =
                        &mut *self.events.handles[&args[0]].borrow_mut()
                    {
                        *signaled = false;
                    }
                }
                Ok(1)
            }
            Call::ResetEvent => {
                if let Object::Event { signaled, .. } =
                    &mut *self.events.handles[&args[0]].borrow_mut()
                {
                    *signaled = false;
                }
                Ok(1)
            }
            Call::CloseHandle => Ok(u32::from(self.events.close(args[0]))),
            Call::WaitForSingleObject => {
                if let Some(value) =
                    acquire(&self.events.handles, &[args[0]], false, self.thread_id)
                {
                    Ok(value)
                } else if args[1] == 0 {
                    Ok(258)
                } else {
                    Err(Error::Host("blocking wait requires scheduleCall".into()))
                }
            }
            _ => Err(Error::Host("incorrect event routing".into())),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/events.rs"]
mod tests;
