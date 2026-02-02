use std::net::TcpListener;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;

use tao::event_loop::EventLoopProxy;
use tiny_http::{Method, Response, Server, StatusCode};

use crate::AppEvent;

const DONE_HOST: &str = "127.0.0.1";
const DONE_PATH: &str = "/done";
const DEFAULT_DONE_PORT: u16 = 9742;

pub fn find_available_port(start_port: u16) -> u16 {
    for port in start_port..=u16::MAX {
        let addr = format!("{DONE_HOST}:{port}");
        if TcpListener::bind(&addr).is_ok() {
            return port;
        }
    }
    DEFAULT_DONE_PORT
}

pub fn spawn_done_server(proxy: EventLoopProxy<AppEvent>, done: Arc<AtomicBool>, port: u16) {
    thread::spawn(move || {
        let address = format!("{DONE_HOST}:{port}");
        let server = match Server::http(&address) {
            Ok(server) => server,
            Err(err) => {
                eprintln!("Failed to start done server on {address}: {err}");
                return;
            }
        };

        for request in server.incoming_requests() {
            let method = request.method();
            let url = request.url();

            if method == &Method::Post && url == DONE_PATH {
                if done.load(Ordering::Relaxed) {
                    let _ = request.respond(Response::empty(StatusCode(409)));
                    continue;
                }

                let _ = proxy.send_event(AppEvent::ExternalDone);
                let _ = request.respond(Response::from_string("ok"));
                continue;
            }

            let status = if url == DONE_PATH {
                StatusCode(405)
            } else {
                StatusCode(404)
            };
            let _ = request.respond(Response::empty(status));
        }
    });
}
