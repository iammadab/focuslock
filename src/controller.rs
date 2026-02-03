use std::time::{Duration, Instant};

use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::ControlFlow;

use crate::AppEvent;

pub struct ControllerResponse {
    pub control_flow: Option<ControlFlow>,
    pub set_done_flag: bool,
    pub timer_text: Option<String>,
    pub pin_mode_started: bool,
    pub pin_mode_ended: bool,
}

pub struct AppState {
    total: Duration,
    start: Option<Instant>,
    loaded: bool,
    next_tick: Instant,
    done: bool,
    flash_until: Option<Instant>,
    pin_notice_until: Option<Instant>,
    escape_key: Option<String>,
    reset_on_focus_loss: bool,
    pin_active: bool,
    pin_buffer: String,
}

impl AppState {
    pub fn new(total: Duration, escape_key: Option<String>, reset_on_focus_loss: bool) -> Self {
        Self {
            total,
            start: None,
            loaded: false,
            next_tick: Instant::now(),
            done: false,
            flash_until: None,
            pin_notice_until: None,
            escape_key,
            reset_on_focus_loss,
            pin_active: false,
            pin_buffer: String::new(),
        }
    }

    pub fn handle_event(&mut self, event: &Event<AppEvent>) -> ControllerResponse {
        let mut response = ControllerResponse {
            control_flow: None,
            set_done_flag: false,
            timer_text: None,
            pin_mode_started: false,
            pin_mode_ended: false,
        };

        match event {
            Event::NewEvents(StartCause::Init) => {
                self.next_tick = Instant::now();
            }
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                self.handle_tick(Instant::now(), &mut response);
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
                if !self.reset_on_focus_loss {
                    return response;
                }
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
            Event::UserEvent(AppEvent::HotkeyNotify) => {
                if self.done || self.pin_active {
                    return response;
                }
                self.pin_active = true;
                self.pin_buffer.clear();
                response.pin_mode_started = true;
                self.set_timer_text(&mut response, "Enter PIN");
            }
            Event::UserEvent(AppEvent::PinDigit(digit)) => {
                if !self.pin_active || self.done {
                    return response;
                }
                let digit = *digit;
                if digit <= 9 {
                    self.pin_buffer.push(char::from(b'0' + digit));
                }
                self.update_pin_text(&mut response);
            }
            Event::UserEvent(AppEvent::PinBackspace) => {
                if !self.pin_active || self.done {
                    return response;
                }
                self.pin_buffer.pop();
                self.update_pin_text(&mut response);
            }
            Event::UserEvent(AppEvent::PinSubmit) => {
                if !self.pin_active || self.done {
                    return response;
                }
                let matches = self
                    .escape_key
                    .as_ref()
                    .map(|key| key == &self.pin_buffer)
                    .unwrap_or(false);
                if matches {
                    self.pin_active = false;
                    self.pin_buffer.clear();
                    self.done = true;
                    response.set_done_flag = true;
                    response.pin_mode_ended = true;
                } else {
                    self.pin_active = false;
                    self.pin_buffer.clear();
                    self.pin_notice_until = Some(Instant::now() + Duration::from_secs(1));
                    response.pin_mode_ended = true;
                    self.handle_tick(Instant::now(), &mut response);
                }
            }
            Event::UserEvent(AppEvent::PinCancel) => {
                if !self.pin_active || self.done {
                    return response;
                }
                self.pin_active = false;
                self.pin_buffer.clear();
                response.pin_mode_ended = true;
                self.handle_tick(Instant::now(), &mut response);
            }
            Event::UserEvent(AppEvent::Tick) => {
                self.handle_tick(Instant::now(), &mut response);
            }
            _ => {}
        }

        response
    }

    fn handle_tick(&mut self, now: Instant, response: &mut ControllerResponse) {
        if self.pin_active {
            self.next_tick = now + Duration::from_secs(1);
            return;
        }
        if let Some(until) = self.pin_notice_until {
            if now < until {
                self.set_timer_text(response, "Incorrect");
                self.next_tick = now + Duration::from_millis(300);
                return;
            }
            self.pin_notice_until = None;
        }
        if self.done {
            response.control_flow = Some(ControlFlow::Wait);
            return;
        }

        if self.start.is_none() {
            response.timer_text = Some("Loading".to_string());
            self.next_tick = now + Duration::from_millis(300);
            return;
        }

        if let Some(until) = self.flash_until {
            if now < until {
                response.timer_text = Some("Timer reset".to_string());
                self.next_tick = now + Duration::from_millis(300);
                return;
            }
            self.flash_until = None;
        }

        let remaining = self.total.saturating_sub(self.start.unwrap().elapsed());
        if remaining.is_zero() {
            self.done = true;
            response.set_done_flag = true;
            response.timer_text = Some("Done".to_string());
            response.control_flow = Some(ControlFlow::Wait);
            return;
        }

        let remaining_secs = remaining.as_secs();
        let minutes = remaining_secs / 60;
        let seconds = remaining_secs % 60;
        let text = format!("{minutes:02}:{seconds:02}");
        response.timer_text = Some(text);
        self.next_tick = now + Duration::from_secs(1);
    }

    fn set_timer_text(&self, response: &mut ControllerResponse, text: &str) {
        response.timer_text = Some(text.to_string());
    }

    fn update_pin_text(&self, response: &mut ControllerResponse) {
        let masked = "*".repeat(self.pin_buffer.len());
        let text = if masked.is_empty() {
            "PIN:".to_string()
        } else {
            format!("PIN: {masked}")
        };
        self.set_timer_text(response, &text);
    }
}
