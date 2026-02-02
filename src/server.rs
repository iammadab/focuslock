use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;

use tao::event_loop::EventLoopProxy;
use tiny_http::{Method, Response, Server, StatusCode};

use crate::AppEvent;

const DONE_ADDRESS: &str = "127.0.0.1:9742";

pub fn spawn_done_server(proxy: EventLoopProxy<AppEvent>, done: Arc<AtomicBool>) {
    thread::spawn(move || {
        let server = match Server::http(DONE_ADDRESS) {
            Ok(server) => server,
            Err(err) => {
                eprintln!("Failed to start done server on {DONE_ADDRESS}: {err}");
                return;
            }
        };

        for request in server.incoming_requests() {
            let method = request.method();
            let url = request.url();

            if method == &Method::Post && url == "/done" {
                if done.load(Ordering::Relaxed) {
                    let _ = request.respond(Response::empty(StatusCode(409)));
                    continue;
                }

                let _ = proxy.send_event(AppEvent::ExternalDone);
                let _ = request.respond(Response::from_string("ok"));
                continue;
            }

            let status = if url == "/done" {
                StatusCode(405)
            } else {
                StatusCode(404)
            };
            let _ = request.respond(Response::empty(status));
        }
    });
}
