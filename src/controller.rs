use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::ControlFlow;

use crate::overlay::{
    set_prompt_message_script, set_timer_script, CLEAR_PROMPT_SCRIPT, HIDE_PROMPT_SCRIPT,
    SHOW_PROMPT_SCRIPT,
};
use crate::AppEvent;

pub enum Script {
    Static(&'static str),
    Owned(String),
}

impl Script {
    pub fn as_str(&self) -> &str {
        match self {
            Script::Static(value) => value,
            Script::Owned(value) => value.as_str(),
        }
    }
}

pub struct ControllerResponse {
    pub control_flow: Option<ControlFlow>,
    pub scripts: Vec<Script>,
    pub set_done_flag: bool,
}

pub struct AppState {
    total: Duration,
    start: Option<Instant>,
    loaded: bool,
    next_tick: Instant,
    done: bool,
    flash_until: Option<Instant>,
    prompt_open: bool,
    escape_key: Option<String>,
}

impl AppState {
    pub fn new(total: Duration, escape_key: Option<String>) -> Self {
        Self {
            total,
            start: None,
            loaded: false,
            next_tick: Instant::now(),
            done: false,
            flash_until: None,
            prompt_open: false,
            escape_key,
        }
    }

    pub fn next_tick(&self) -> Instant {
        self.next_tick
    }

    pub fn handle_event(&mut self, event: &Event<AppEvent>) -> ControllerResponse {
        let mut response = ControllerResponse {
            control_flow: None,
            scripts: Vec::new(),
            set_done_flag: false,
        };

        match event {
            Event::NewEvents(StartCause::Init) => {
                self.next_tick = Instant::now();
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                if self.done {
                    response.control_flow = Some(ControlFlow::Wait);
                    return response;
                }

                let now = Instant::now();
                if self.start.is_none() {
                    response
                        .scripts
                        .push(Script::Owned(set_timer_script("Loading")));
                    self.next_tick = now + Duration::from_millis(300);
                    return response;
                }

                if let Some(until) = self.flash_until {
                    if now < until {
                        response
                            .scripts
                            .push(Script::Owned(set_timer_script("Timer reset")));
                        self.next_tick = now + Duration::from_millis(300);
                        return response;
                    }
                    self.flash_until = None;
                }

                let remaining = self.total.saturating_sub(self.start.unwrap().elapsed());
                if remaining.is_zero() {
                    self.done = true;
                    response.set_done_flag = true;
                    response
                        .scripts
                        .push(Script::Owned(set_timer_script("Done")));
                    response.control_flow = Some(ControlFlow::Wait);
                    return response;
                }

                let remaining_secs = remaining.as_secs();
                let text = if remaining_secs >= 3600 {
                    let hours = remaining_secs / 3600;
                    let minutes = (remaining_secs % 3600) / 60;
                    let seconds = remaining_secs % 60;
                    format!("{hours:02}:{minutes:02}:{seconds:02}")
                } else {
                    let minutes = remaining_secs / 60;
                    let seconds = remaining_secs % 60;
                    format!("{minutes:02}:{seconds:02}")
                };
                response
                    .scripts
                    .push(Script::Owned(set_timer_script(&text)));
                self.next_tick = now + Duration::from_secs(1);
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                if self.done {
                    response.control_flow = Some(ControlFlow::Exit);
                }
            }
            Event::UserEvent(AppEvent::FocusLost) => {
                if self.done || !self.loaded {
                    return response;
                }
                let now = Instant::now();
                self.start = Some(now);
                self.flash_until = Some(now + Duration::from_secs(2));
                self.next_tick = now;
            }
            Event::UserEvent(AppEvent::PageLoaded) => {
                if self.done || self.loaded {
                    return response;
                }
                let now = Instant::now();
                self.loaded = true;
                self.start = Some(now);
                self.next_tick = now;
            }
            Event::UserEvent(AppEvent::EscapeOpen) => {
                if self.done || self.prompt_open {
                    return response;
                }
                if self.escape_key.is_none() {
                    return response;
                }
                self.prompt_open = true;
                response.scripts.push(Script::Static(SHOW_PROMPT_SCRIPT));
            }
            Event::UserEvent(AppEvent::EscapeCancel) => {
                if !self.prompt_open {
                    return response;
                }
                self.prompt_open = false;
                response.scripts.push(Script::Static(HIDE_PROMPT_SCRIPT));
            }
            Event::UserEvent(AppEvent::EscapeSubmit(pin)) => {
                if !self.prompt_open {
                    return response;
                }
                let Some(expected) = self.escape_key.as_ref() else {
                    return response;
                };
                if pin == expected {
                    self.prompt_open = false;
                    self.done = true;
                    response.set_done_flag = true;
                    response.scripts.push(Script::Static(HIDE_PROMPT_SCRIPT));
                    response
                        .scripts
                        .push(Script::Owned(set_timer_script("Unlocked")));
                    response.control_flow = Some(ControlFlow::Wait);
                } else {
                    response
                        .scripts
                        .push(Script::Owned(set_prompt_message_script("Incorrect PIN")));
                    response.scripts.push(Script::Static(CLEAR_PROMPT_SCRIPT));
                }
            }
            _ => {}
        }

        response
    }
}
