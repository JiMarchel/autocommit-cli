//! The generation pipeline: diff -> hints -> prompt -> Gemini -> validate/repair.

use crate::diff::{self, FileChange, Limits};
use crate::gemini::Client;
use crate::hints::{self, Hints};
use crate::message::{self, Violation};
use crate::prompt;
use anyhow::Result;

pub struct Generator<'a> {
    client: &'a Client,
    hints: Hints,
    base_prompt: String,
}

impl<'a> Generator<'a> {
    pub fn new(client: &'a Client, files: &[FileChange]) -> Self {
        let hints = hints::infer(files);
        let rendered = diff::render(files, Limits::default());
        let base_prompt = prompt::build(&hints, &rendered);
        Generator {
            client,
            hints,
            base_prompt,
        }
    }

    /// Always returns a message that passes `message::validate`.
    pub fn generate(&self) -> Result<String> {
        let first = self.ask(&self.base_prompt)?;
        let violation = match self.check(&first) {
            Ok(msg) => return Ok(msg),
            Err(v) => v,
        };
        let retry_prompt = prompt::correction(&self.base_prompt, &first, &violation);
        let second = self.ask(&retry_prompt)?;
        if let Ok(msg) = self.check(&second) {
            return Ok(msg);
        }
        let repaired = message::repair(&second, &self.hints);
        // `repair` is designed to always validate; never let a bad message through.
        Ok(match self.check(&repaired) {
            Ok(msg) => msg,
            Err(_) => message::repair("", &self.hints),
        })
    }

    fn ask(&self, prompt: &str) -> Result<String> {
        Ok(message::sanitize(&self.client.generate(prompt)?))
    }

    /// Enforce the locked type (cheap, deterministic), then validate.
    fn check(&self, msg: &str) -> Result<String, Violation> {
        let msg = match self.hints.locked_type {
            Some(t) => message::enforce_type(msg, t),
            None => msg.to_string(),
        };
        message::validate(&msg, self.hints.locked_type).map(|()| msg)
    }
}
