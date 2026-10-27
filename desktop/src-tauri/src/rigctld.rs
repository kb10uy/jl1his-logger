use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

use thiserror::Error;

const MAXIMUM_RESPONSE_LINES: usize = 512;
const MAXIMUM_RESPONSE_LINE_BYTES: u64 = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RigStatus {
    pub frequency_hz: u64,
    pub mode: String,
}

#[derive(Debug)]
pub struct Rigctld {
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    requires_vfo: bool,
}

impl Rigctld {
    pub fn connect(address: &str, timeout: Duration) -> Result<Self, RigError> {
        let target = address
            .to_socket_addrs()
            .map_err(|_| RigError::Address(address.to_owned()))?
            .next()
            .ok_or_else(|| RigError::Address(address.to_owned()))?;
        let connect = |error: std::io::Error| RigError::Connect {
            address: address.to_owned(),
            detail: error.to_string(),
        };
        let stream = TcpStream::connect_timeout(&target, timeout).map_err(connect)?;
        stream.set_read_timeout(Some(timeout)).map_err(connect)?;
        stream.set_write_timeout(Some(timeout)).map_err(connect)?;
        stream.set_nodelay(true).map_err(connect)?;
        let writer = stream.try_clone().map_err(connect)?;
        let mut rig = Self {
            reader: BufReader::new(stream),
            writer,
            requires_vfo: false,
        };
        rig.requires_vfo = rig.asks_for_a_vfo()?;
        Ok(rig)
    }

    #[cfg(test)]
    pub const fn requires_vfo(&self) -> bool {
        self.requires_vfo
    }

    pub fn frequency(&mut self) -> Result<u64, RigError> {
        let lines = self.query("\\get_freq")?;
        number(&lines).ok_or_else(|| unreadable("\\get_freq"))
    }

    pub fn model(&mut self) -> Option<String> {
        let lines = self.send("\\dump_caps").ok()?;
        labelled(&lines, "Model name").map(str::to_owned)
    }

    pub fn mode(&mut self) -> Result<String, RigError> {
        let lines = self.query("\\get_mode")?;
        labelled(&lines, "mode")
            .map(str::to_owned)
            .ok_or_else(|| unreadable("\\get_mode"))
    }

    pub fn status(&mut self) -> Result<RigStatus, RigError> {
        Ok(RigStatus {
            frequency_hz: self.frequency()?,
            mode: self.mode()?,
        })
    }

    fn query(&mut self, name: &str) -> Result<Vec<String>, RigError> {
        if self.requires_vfo {
            self.send(&format!("{name} currVFO"))
        } else {
            self.send(name)
        }
    }

    fn asks_for_a_vfo(&mut self) -> Result<bool, RigError> {
        match self.send("\\chk_vfo") {
            Ok(lines) => Ok(number(&lines) == Some(1)),
            Err(error) if error.is_fatal() => Err(error),
            Err(_) => Ok(false),
        }
    }

    fn send(&mut self, command: &str) -> Result<Vec<String>, RigError> {
        writeln!(self.writer, "+{command}")
            .and_then(|()| self.writer.flush())
            .map_err(|error| RigError::Transport(error.to_string()))?;
        let mut lines = Vec::new();
        loop {
            let mut raw = String::new();
            let read = (&mut self.reader)
                .take(MAXIMUM_RESPONSE_LINE_BYTES)
                .read_line(&mut raw)
                .map_err(|error| RigError::Transport(error.to_string()))?;
            if read == 0 {
                return Err(RigError::Closed);
            }
            if read as u64 == MAXIMUM_RESPONSE_LINE_BYTES && !raw.ends_with('\n') {
                return Err(unreadable(command));
            }
            let line = raw.trim_end_matches(['\r', '\n']);
            if let Some(status) = line.strip_prefix("RPRT ") {
                let code: i32 = status.trim().parse().map_err(|_| unreadable(command))?;
                if code != 0 {
                    return Err(RigError::Refused {
                        command: command.to_owned(),
                        code,
                    });
                }
                return Ok(lines);
            }
            if lines.len() >= MAXIMUM_RESPONSE_LINES {
                return Err(unreadable(command));
            }
            lines.push(line.to_owned());
        }
    }
}

impl Drop for Rigctld {
    fn drop(&mut self) {
        let mut answer = String::new();
        let _ = writeln!(self.writer, "q")
            .and_then(|()| self.writer.flush())
            .and_then(|()| self.reader.read_line(&mut answer));
    }
}

fn number(lines: &[String]) -> Option<u64> {
    lines
        .iter()
        .find_map(|line| line.rsplit(':').next().and_then(|value| value.trim().parse().ok()))
}

fn labelled<'a>(lines: &'a [String], name: &str) -> Option<&'a str> {
    lines.iter().find_map(|line| {
        let (label, value) = line.split_once(':')?;
        label
            .trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim())
            .filter(|value| !value.is_empty())
    })
}

fn unreadable(command: &str) -> RigError {
    RigError::Unreadable {
        command: command.to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RigError {
    #[error("`{0}` is not an address rigctld can be reached at")]
    Address(String),
    #[error("rigctld could not be reached at {address}: {detail}")]
    Connect { address: String, detail: String },
    #[error("the connection to rigctld failed: {0}")]
    Transport(String),
    #[error("rigctld closed the connection")]
    Closed,
    #[error("`{command}` was refused by the rig with status {code}")]
    Refused { command: String, code: i32 },
    #[error("rigctld answered `{command}` with nothing readable")]
    Unreadable { command: String },
}

impl RigError {
    pub const fn is_fatal(&self) -> bool {
        matches!(
            self,
            Self::Address(_) | Self::Connect { .. } | Self::Transport(_) | Self::Closed
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::BufWriter,
        net::TcpListener,
        sync::{Arc, Mutex},
        thread::{self, JoinHandle},
    };

    use super::*;

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);
    const NO_VFO: &str = "chk_vfo:\nChkVFO: 0\nRPRT 0\n";

    struct FakeRig {
        address: String,
        received: Arc<Mutex<Vec<String>>>,
        thread: Option<JoinHandle<()>>,
    }

    impl FakeRig {
        fn spawn(answers: &[&str]) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap().to_string();
            let received = Arc::new(Mutex::new(Vec::new()));
            let recorder = Arc::clone(&received);
            let answers: Vec<String> = answers.iter().map(|answer| (*answer).to_owned()).collect();
            let thread = thread::spawn(move || {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let mut writer = BufWriter::new(stream.try_clone().unwrap());
                let reader = BufReader::new(stream);
                for (request, answer) in reader.lines().map_while(Result::ok).zip(answers) {
                    recorder.lock().unwrap().push(request);
                    if writer.write_all(answer.as_bytes()).is_err() || writer.flush().is_err() {
                        return;
                    }
                }
            });
            Self {
                address,
                received,
                thread: Some(thread),
            }
        }

        fn received(&self) -> Vec<String> {
            self.received.lock().unwrap().clone()
        }
    }

    impl Drop for FakeRig {
        fn drop(&mut self) {
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    #[test]
    fn reads_the_frequency() {
        let fake = FakeRig::spawn(&[NO_VFO, "get_freq:\nFreq: 7041500\nRPRT 0\n"]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert_eq!(rig.frequency(), Ok(7_041_500));
        assert!(!rig.requires_vfo());
        assert_eq!(fake.received(), ["+\\chk_vfo", "+\\get_freq"]);
    }

    #[test]
    fn reads_the_model() {
        let fake = FakeRig::spawn(&[
            NO_VFO,
            "dump_caps:\nCaps dump for model: 3085\nModel name:\tIC-705\nMfg name:\tIcom\nRPRT 0\n",
        ]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert_eq!(rig.model(), Some("IC-705".to_owned()));
    }

    #[test]
    fn reads_the_mode() {
        let fake = FakeRig::spawn(&[NO_VFO, "get_mode:\nMode: CW\nPassband: 500\nRPRT 0\n"]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert_eq!(rig.mode(), Ok("CW".to_owned()));
    }

    #[test]
    fn reads_the_frequency_and_the_mode_together() {
        let fake = FakeRig::spawn(&[
            NO_VFO,
            "get_freq:\nFreq: 14074000\nRPRT 0\n",
            "get_mode:\nMode: PKTUSB\nPassband: 3000\nRPRT 0\n",
        ]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert_eq!(
            rig.status(),
            Ok(RigStatus {
                frequency_hz: 14_074_000,
                mode: "PKTUSB".to_owned(),
            })
        );
    }

    #[test]
    fn addresses_a_vfo_when_rigctld_asks_for_one() {
        let fake = FakeRig::spawn(&[
            "chk_vfo:\nChkVFO: 1\nRPRT 0\n",
            "get_freq: currVFO\nFreq: 7178000\nRPRT 0\n",
        ]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert!(rig.requires_vfo());
        assert_eq!(rig.frequency(), Ok(7_178_000));
        assert_eq!(fake.received(), ["+\\chk_vfo", "+\\get_freq currVFO"]);
    }

    #[test]
    fn settles_on_no_vfo_when_the_question_is_refused() {
        let fake = FakeRig::spawn(&["RPRT -11\n", "get_freq:\nFreq: 50110000\nRPRT 0\n"]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert!(!rig.requires_vfo());
        assert_eq!(rig.frequency(), Ok(50_110_000));
    }

    #[test]
    fn reports_the_status_the_rig_refused_with() {
        let fake = FakeRig::spawn(&[NO_VFO, "get_mode:\nRPRT -9\n"]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        let refused = rig.mode().unwrap_err();

        assert_eq!(
            refused,
            RigError::Refused {
                command: "\\get_mode".to_owned(),
                code: -9,
            }
        );
        assert!(!refused.is_fatal());
    }

    #[test]
    fn reports_an_answer_without_a_frequency() {
        let fake = FakeRig::spawn(&[NO_VFO, "get_freq:\nFreq: ?\nRPRT 0\n"]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert_eq!(rig.frequency(), Err(unreadable("\\get_freq")));
    }

    #[test]
    fn refuses_an_unframed_answer_at_a_fixed_size() {
        let unframed = "x".repeat(MAXIMUM_RESPONSE_LINE_BYTES as usize * 2);
        let fake = FakeRig::spawn(&[NO_VFO, &unframed]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        assert_eq!(rig.frequency(), Err(unreadable("\\get_freq")));
    }

    #[test]
    fn says_goodbye_when_dropped() {
        let fake = FakeRig::spawn(&[NO_VFO, "RPRT 0\n"]);
        let rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();
        let received = Arc::clone(&fake.received);

        drop(rig);
        drop(fake);

        assert_eq!(*received.lock().unwrap(), ["+\\chk_vfo", "q"]);
    }

    #[test]
    fn reports_a_hangup() {
        let fake = FakeRig::spawn(&[NO_VFO]);
        let mut rig = Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap();

        let error = rig.frequency().unwrap_err();

        assert_eq!(error, RigError::Closed);
        assert!(error.is_fatal());
    }

    #[test]
    fn reports_a_hangup_while_connecting() {
        let fake = FakeRig::spawn(&[]);

        assert_eq!(
            Rigctld::connect(&fake.address, TEST_TIMEOUT).unwrap_err(),
            RigError::Closed
        );
    }

    #[test]
    fn reports_nothing_listening_against_the_address() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap().to_string();
        drop(listener);

        let error = Rigctld::connect(&address, Duration::from_millis(200)).unwrap_err();

        assert!(matches!(error, RigError::Connect { .. }), "{error:?}");
    }

    #[test]
    fn reports_an_unresolvable_address() {
        assert_eq!(
            Rigctld::connect("not a host", TEST_TIMEOUT).unwrap_err(),
            RigError::Address("not a host".to_owned())
        );
    }
}
