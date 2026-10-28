use std::{
    net::SocketAddr,
    time::{Duration, Instant},
};

const EXPIRY: Duration = Duration::from_secs(15);
const MAX_SUBSCRIBERS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subscriber {
    pub address: SocketAddr,
    pub listener: usize,
    at: Instant,
}

#[derive(Debug, Default)]
pub struct Subscribers(Vec<Subscriber>);

impl Subscribers {
    pub fn subscribe(&mut self, address: SocketAddr, listener: usize, now: Instant) -> bool {
        if !address.ip().to_canonical().is_loopback() {
            return false;
        }
        self.expire(now);
        if let Some(known) = self.0.iter_mut().find(|s| s.address == address) {
            known.listener = listener;
            known.at = now;
            return true;
        }
        if self.0.len() >= MAX_SUBSCRIBERS {
            return false;
        }
        self.0.push(Subscriber {
            address,
            listener,
            at: now,
        });
        true
    }

    pub fn unsubscribe(&mut self, address: SocketAddr) {
        self.0.retain(|s| s.address != address);
    }

    pub fn expire(&mut self, now: Instant) {
        self.0.retain(|s| now.duration_since(s.at) < EXPIRY);
    }

    pub fn forget_listener(&mut self, listener: usize) {
        self.0.retain(|s| s.listener != listener);
    }

    pub fn take(&mut self) -> Vec<Subscriber> {
        std::mem::take(&mut self.0)
    }

    pub fn count(&self, listener: usize) -> usize {
        self.0.iter().filter(|s| s.listener == listener).count()
    }

    pub fn all(&self) -> &[Subscriber] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    fn at(address: &str) -> SocketAddr {
        address.parse().unwrap()
    }

    fn listed(subscribers: &Subscribers) -> Vec<(String, usize)> {
        subscribers
            .all()
            .iter()
            .map(|s| (s.address.to_string(), s.listener))
            .collect()
    }

    #[test]
    fn registers_and_renews_a_subscriber() {
        let mut subscribers = Subscribers::default();
        let now = Instant::now();
        assert!(subscribers.subscribe(at("127.0.0.1:50000"), 0, now));
        assert!(subscribers.subscribe(at("127.0.0.1:50001"), 0, now));
        assert!(subscribers.subscribe(at("127.0.0.1:50000"), 1, now + Duration::from_secs(10)));
        assert_eq!(
            listed(&subscribers),
            [("127.0.0.1:50000".to_owned(), 1), ("127.0.0.1:50001".to_owned(), 0)]
        );
        assert_eq!((subscribers.count(0), subscribers.count(1)), (1, 1));
        subscribers.expire(now + EXPIRY);
        assert_eq!(listed(&subscribers), [("127.0.0.1:50000".to_owned(), 1)]);
        subscribers.unsubscribe(at("127.0.0.1:50000"));
        assert!(subscribers.all().is_empty());
    }

    #[test]
    fn refuses_new_subscribers_past_the_limit() {
        let mut subscribers = Subscribers::default();
        let now = Instant::now();
        for port in 0..MAX_SUBSCRIBERS {
            assert!(subscribers.subscribe(at(&format!("127.0.0.1:{}", 50000 + port)), 0, now));
        }
        assert!(!subscribers.subscribe(at("127.0.0.1:60000"), 0, now));
        assert!(subscribers.subscribe(at("127.0.0.1:50000"), 0, now));
        assert!(subscribers.subscribe(at("127.0.0.1:60000"), 0, now + EXPIRY));
        assert_eq!(subscribers.all().len(), 1);
    }

    #[rstest]
    #[case("127.0.0.1:50000", true)]
    #[case("127.1.2.3:50000", true)]
    #[case("[::1]:50000", true)]
    #[case("[::ffff:127.0.0.1]:50000", true)]
    #[case("192.168.1.10:50000", false)]
    #[case("[::ffff:192.168.1.10]:50000", false)]
    #[case("[2001:db8::1]:50000", false)]
    fn accepts_only_the_loopback_address(#[case] address: &str, #[case] accepted: bool) {
        let mut subscribers = Subscribers::default();
        assert_eq!(subscribers.subscribe(at(address), 0, Instant::now()), accepted);
        assert_eq!(subscribers.all().len(), usize::from(accepted));
    }

    #[test]
    fn forgets_the_subscribers_of_a_listener() {
        let mut subscribers = Subscribers::default();
        let now = Instant::now();
        subscribers.subscribe(at("127.0.0.1:50000"), 0, now);
        subscribers.subscribe(at("127.0.0.1:50001"), 1, now);
        subscribers.forget_listener(0);
        assert_eq!(listed(&subscribers), [("127.0.0.1:50001".to_owned(), 1)]);
        assert_eq!(subscribers.take().len(), 1);
        assert!(subscribers.all().is_empty());
    }
}
