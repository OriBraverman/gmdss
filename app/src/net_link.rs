//! The classroom network: the instructor's console listens, students connect; messages are the
//! text lines of `gmdss::net::Msg`. One reader thread per connection feeds a channel the app
//! polls each frame.

use gmdss::net::Msg;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{mpsc, Arc, Mutex};

#[derive(Clone, Debug, PartialEq)]
pub struct Student {
    pub id: usize,
    pub name: String,
    pub mmsi: String,
    pub pos: (i32, i32),
}

pub struct Link {
    pub instructor: bool,
    /// Where we listen or what we are connected to.
    pub address: String,
    inbox: mpsc::Receiver<(usize, Option<Msg>)>,
    peers: Arc<Mutex<Vec<(usize, TcpStream)>>>,
    /// The instructor's class list.
    pub students: Vec<Student>,
}

fn read(id: usize, stream: TcpStream, tx: mpsc::Sender<(usize, Option<Msg>)>) {
    std::thread::spawn(move || {
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            if let Some(m) = Msg::from_line(&line) {
                if tx.send((id, Some(m))).is_err() {
                    return;
                }
            }
        }
        let _ = tx.send((id, None));
    });
}

impl Link {
    /// Listens on `port` (0: any free port) for students.
    pub fn instructor(port: u16) -> std::io::Result<Link> {
        let listener = TcpListener::bind(("0.0.0.0", port))?;
        let address = format!("port {}", listener.local_addr()?.port());
        let (tx, inbox) = mpsc::channel();
        let peers: Arc<Mutex<Vec<(usize, TcpStream)>>> = Arc::default();
        let accepted = peers.clone();
        std::thread::spawn(move || {
            for (id, stream) in listener.incoming().flatten().enumerate() {
                let Ok(reader) = stream.try_clone() else { continue };
                accepted.lock().unwrap().push((id, stream));
                read(id, reader, tx.clone());
            }
        });
        Ok(Link { instructor: true, address, inbox, peers, students: vec![] })
    }

    /// Connects to the instructor at `addr` (host:port).
    pub fn student(addr: &str) -> std::io::Result<Link> {
        let stream = TcpStream::connect(addr)?;
        let (tx, inbox) = mpsc::channel();
        read(0, stream.try_clone()?, tx);
        Ok(Link { instructor: false, address: addr.to_string(), inbox, peers: Arc::new(Mutex::new(vec![(0, stream)])), students: vec![] })
    }

    #[cfg(test)]
    pub fn local_port(&self) -> Option<u16> {
        self.address.strip_prefix("port ")?.parse().ok()
    }

    /// Sends to every peer except `except`; peers that fail are dropped.
    pub fn send(&self, msg: &Msg, except: Option<usize>) {
        let line = msg.to_line() + "\n";
        self.peers.lock().unwrap().retain_mut(|(id, s)| Some(*id) == except || s.write_all(line.as_bytes()).is_ok());
    }

    pub fn connected(&self) -> usize {
        self.peers.lock().unwrap().len()
    }

    /// Messages received since the last call; keeps the class list up to date.
    pub fn poll(&mut self) -> Vec<(usize, Msg)> {
        let mut out = vec![];
        while let Ok((id, msg)) = self.inbox.try_recv() {
            match msg {
                None => {
                    self.students.retain(|s| s.id != id);
                    self.peers.lock().unwrap().retain(|(p, _)| *p != id);
                }
                Some(Msg::Hello { name, mmsi, lat_min, lon_min }) => {
                    let s = Student { id, name: name.clone(), mmsi: mmsi.clone(), pos: (lat_min, lon_min) };
                    match self.students.iter_mut().find(|s| s.id == id) {
                        Some(old) => *old = s,
                        None => self.students.push(s),
                    }
                    out.push((id, Msg::Hello { name, mmsi, lat_min, lon_min }));
                }
                Some(m) => out.push((id, m)),
            }
        }
        out
    }
}

impl Drop for Link {
    /// Closes the connections (the reader threads hold their own handles).
    fn drop(&mut self) {
        for (_, s) in self.peers.lock().unwrap().iter() {
            let _ = s.shutdown(std::net::Shutdown::Both);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn wait(link: &mut Link, n: usize) -> Vec<(usize, Msg)> {
        wait_for(link, n, 5.0)
    }

    fn wait_for(link: &mut Link, n: usize, secs: f64) -> Vec<(usize, Msg)> {
        let start = Instant::now();
        let mut got = vec![];
        while got.len() < n && start.elapsed() < Duration::from_secs_f64(secs) {
            got.extend(link.poll());
            std::thread::sleep(Duration::from_millis(10));
        }
        got
    }

    /// An instructor and two students on this machine: hello, an exercise to all, a relayed call.
    #[test]
    fn classroom() {
        let mut teacher = Link::instructor(0).unwrap();
        let addr = format!("127.0.0.1:{}", teacher.local_port().unwrap());
        let mut a = Link::student(&addr).unwrap();
        let mut b = Link::student(&addr).unwrap();
        a.send(&Msg::Hello { name: "ALPHA".into(), mmsi: "211000001".into(), lat_min: 1970, lon_min: 2080 }, None);
        b.send(&Msg::Hello { name: "BRAVO".into(), mmsi: "211000002".into(), lat_min: 1980, lon_min: 2090 }, None);
        let hellos = wait(&mut teacher, 2);
        assert_eq!(hellos.len(), 2);
        assert_eq!(teacher.students.len(), 2);
        teacher.send(&Msg::Exercise("two_tone".into()), None);
        assert_eq!(wait(&mut a, 1)[0].1, Msg::Exercise("two_tone".into()));
        assert_eq!(wait(&mut b, 1)[0].1, Msg::Exercise("two_tone".into()));
        // A's distress alert goes to the instructor, who relays it to everyone but A.
        let call = gmdss::dsc::Call::distress("211000001", gmdss::dsc::Nature::Sinking, None, None);
        a.send(&Msg::Dsc { lat_min: 1970, lon_min: 2080, call: call.clone() }, None);
        let (from, msg) = wait(&mut teacher, 1).remove(0);
        teacher.send(&msg, Some(from));
        assert_eq!(wait(&mut b, 1)[0].1, Msg::Dsc { lat_min: 1970, lon_min: 2080, call });
        assert!(wait_for(&mut a, 1, 0.3).is_empty());
        drop(a);
        wait(&mut teacher, 1);
        assert_eq!(teacher.students.len(), 1);
    }
}
