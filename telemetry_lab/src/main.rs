use std::io;
use std::net::{SocketAddr, UdpSocket};
use std::thread;
use std::time::{Duration, Instant};

const ALPHA: f64 = 0.2;

struct Target {
    port_id: u8,
    peer: SocketAddr,
    estimate: Estimate,
}

impl Target {
    fn new(port_id: u8, address: &str) -> Self {
        let peer = address.parse().expect("parse peer address");
        Self {
            port_id,
            peer,
            estimate: Estimate::new(),
        }
    }
}

struct Estimate {
    rtt_ms: Option<f64>,
    loss: f64,
}

impl Estimate {
    fn new() -> Self {
        Self {
            rtt_ms: None,
            loss: 0.0,
        }
    }

    fn observe(&mut self, sample: Option<f64>) {
        match sample {
            Some(rtt_ms) => {
                self.rtt_ms = Some(match self.rtt_ms {
                    Some(previous) => ewma(previous, rtt_ms),
                    None => rtt_ms,
                });
                self.loss = ewma(self.loss, 0.0);
            }
            None => {
                self.loss = ewma(self.loss, 1.0);
            }
        }
    }

    fn score(&self) -> f64 {
        let rtt_ms = self.rtt_ms.unwrap_or(0.0);
        rtt_ms + self.loss * 1_000.0
    }
}

fn ewma(previous: f64, sample: f64) -> f64 {
    ALPHA * sample + (1.0 - ALPHA) * previous
}

fn main() {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind learner");
    socket
        .set_read_timeout(Some(Duration::from_millis(200)))
        .expect("set probe timeout");

    let mut targets = vec![
        Target::new(1, "127.0.0.1:5201"),
        Target::new(2, "127.0.0.1:5202"),
        Target::new(3, "127.0.0.1:5203"),
    ];

    loop {
        for target in &mut targets {
            match probe(&socket, target.peer) {
                Some(rtt_ms) => target.estimate.observe(Some(rtt_ms)),
                None => target.estimate.observe(None),
            }
            println!(
                "port {} | ewma={:.2} ms | loss={:.2} | score={:.1}",
                target.port_id,
                target.estimate.rtt_ms.unwrap_or(0.0),
                target.estimate.loss,
                target.estimate.score()
            );
        }
        let mut best = &targets[0];
        for target in &targets {
            if target.estimate.score() < best.estimate.score() {
                best = target;
            }
        }
        println!("best port: {}", best.port_id);
        println!("---");
        thread::sleep(Duration::from_millis(500));
    }
}

fn probe(socket: &UdpSocket, peer: SocketAddr) -> Option<f64> {
    let mut buf = [0u8; 32];
    let started = Instant::now();

    if socket.send_to(b"HCI-PING", peer).is_err() {
        return None;
    }

    loop {
        match socket.recv_from(&mut buf) {
            Ok((n, from)) => {
                if from == peer && &buf[..n] == b"HCI-PONG" {
                    return Some(started.elapsed().as_secs_f64() * 1_000.0);
                }
            }
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                return None;
            }
            Err(_) => return None,
        }
    }
}
