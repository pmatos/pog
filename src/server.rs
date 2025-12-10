use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use crate::commands::{parse_command, CommandResponse, PogCommand};

pub struct CommandRequest {
    pub command: PogCommand,
    pub response_tx: mpsc::Sender<CommandResponse>,
}

const MAX_PORT_ATTEMPTS: u16 = 100;

fn try_bind_port(starting_port: u16) -> std::io::Result<(TcpListener, u16)> {
    for offset in 0..MAX_PORT_ATTEMPTS {
        let port = starting_port.saturating_add(offset);
        match TcpListener::bind(format!("127.0.0.1:{}", port)) {
            Ok(listener) => return Ok((listener, port)),
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                continue;
            }
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AddrInUse,
        format!(
            "could not find available port in range {}-{}",
            starting_port,
            starting_port.saturating_add(MAX_PORT_ATTEMPTS - 1)
        ),
    ))
}

pub fn start_server(
    port: u16,
    command_tx: async_channel::Sender<CommandRequest>,
) -> std::io::Result<JoinHandle<()>> {
    let (listener, actual_port) = try_bind_port(port)?;
    eprintln!("pog server listening on 127.0.0.1:{}", actual_port);

    let handle = thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let command_tx = command_tx.clone();
                    thread::spawn(move || {
                        handle_client(stream, command_tx);
                    });
                }
                Err(e) => {
                    eprintln!("Connection error: {}", e);
                }
            }
        }
    });

    Ok(handle)
}

fn handle_client(mut stream: TcpStream, command_tx: async_channel::Sender<CommandRequest>) {
    let peer = stream
        .peer_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let reader = match stream.try_clone() {
        Ok(s) => BufReader::new(s),
        Err(e) => {
            eprintln!("Failed to clone stream for {}: {}", peer, e);
            return;
        }
    };

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("Read error from {}: {}", peer, e);
                break;
            }
        };

        if line.is_empty() {
            continue;
        }

        let response = match parse_command(&line) {
            Ok(cmd) => {
                let (response_tx, response_rx) = mpsc::channel();
                let request = CommandRequest {
                    command: cmd,
                    response_tx,
                };

                if command_tx.send_blocking(request).is_err() {
                    CommandResponse::Error("UI not available".to_string())
                } else {
                    match response_rx.recv() {
                        Ok(resp) => resp,
                        Err(_) => CommandResponse::Error("no response from UI".to_string()),
                    }
                }
            }
            Err(e) => CommandResponse::Error(e),
        };

        let response_str = format!("{}\n", response);
        if let Err(e) = stream.write_all(response_str.as_bytes()) {
            eprintln!("Write error to {}: {}", peer, e);
            break;
        }
        if let Err(e) = stream.flush() {
            eprintln!("Flush error to {}: {}", peer, e);
            break;
        }
    }
}
