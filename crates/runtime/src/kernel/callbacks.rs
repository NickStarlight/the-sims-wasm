//! Callback-aware call entry points; ordinary API calls retain the synchronous path.
use super::*;
use crate::CallOutcome;

impl Kernel {
    pub fn begin_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        id: u32,
        args: &[u32],
    ) -> Result<CallOutcome> {
        let outcome = self.begin_call_inner(process, id, args)?;
        self.own_callback(&outcome);
        Ok(outcome)
    }
    fn own_callback(&mut self, outcome: &CallOutcome) {
        if let CallOutcome::Callback { token, .. } = outcome {
            self.callback_owners.insert(*token, self.thread_id);
        }
    }
    fn begin_call_inner<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        id: u32,
        args: &[u32],
    ) -> Result<CallOutcome> {
        let api = manifest()
            .get(id as usize)
            .ok_or_else(|| Error::Host(format!("unknown kernel API id {id}")))?;
        if !api.callback {
            return self
                .call(process, id, args)
                .map(|value| CallOutcome::Return { value });
        }
        if args.len() != api.arity as usize {
            return Err(Error::Host(format!(
                "{} expects {} arguments, got {}",
                api.name,
                api.arity,
                args.len()
            )));
        }
        match api.call {
            Call::D3DEnumDevices
            | Call::D3DEnumZBufferFormats
            | Call::D3DDeviceEnumTextureFormats => {
                let calls = self.graphics.d3d_enumerate(process, api.name, args)?;
                self.callbacks.sequence(calls, true, 0)
            }
            Call::EnumSystemLocalesA => self.enumerate_locales(process, args),
            Call::DirectDrawEnumerateA | Call::DDrawEnumDisplayModes => {
                let calls = self.graphics.enumerate(process, api.name, args)?;
                self.callbacks.sequence(calls, true, 0)
            }
            Call::Initterm => self.callbacks.initializers(process, args[0], args[1]),
            Call::DefWindowProcA
            | Call::SetWindowPos
            | Call::WindowProcA
            | Call::CreateWindowExA
            | Call::DestroyWindow
            | Call::SetCapture
            | Call::ReleaseCapture
            | Call::DispatchMessageA
            | Call::SendMessageA => {
                let action = self.user.begin(process, api.name, args)?;
                let outcome = self.callbacks.user_action(&mut self.user, action);
                if let Some(error) = self.user.take_error() {
                    self.thread().error = error;
                }
                outcome
            }
            _ => Err(Error::Host("unsupported callback-capable API".into())),
        }
    }

    pub fn resume_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        token: u32,
        result: u32,
    ) -> Result<CallOutcome> {
        if self.callback_owners.get(&token) != Some(&self.thread_id) {
            return Err(Error::Host("callback belongs to another thread".into()));
        }
        self.callback_owners.remove(&token);
        let outcome = self
            .callbacks
            .resume_user(process, &mut self.user, token, result);
        if let Some(error) = self.user.take_error() {
            self.thread().error = error;
        }
        if let Ok(ref value) = outcome {
            self.own_callback(value);
        }
        outcome
    }

    /// Idempotent cleanup used when the host callback throws or execution is cancelled.
    pub fn cancel_call(&mut self, token: u32) -> bool {
        if self.callback_owners.get(&token) != Some(&self.thread_id) {
            return false;
        }
        self.callback_owners.remove(&token);
        self.callbacks.cancel_user(&mut self.user, token)
    }
}
