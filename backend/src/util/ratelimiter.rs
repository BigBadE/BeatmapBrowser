use chrono::{DateTime, TimeDelta, Utc};
use linked_hash_set::LinkedHashSet;
use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::ops::Add;
use std::time::SystemTime;
use crate::api::APIError;

pub struct Ratelimiter {
    limits: HashMap<SiteAction, Limits>,
}

impl Ratelimiter {
    pub fn new() -> Self {
        let mut limits = HashMap::new();
        for action in ACTIONS {
            limits.insert(
                action,
                Limits {
                    blocked_times: vec![],
                    searchable_users: Default::default(),
                },
            );
        }
        Ratelimiter { limits }
    }

    pub fn check_limited(&mut self, action: SiteAction, ip: &UniqueIdentifier) -> Result<(), APIError> {
        let limit = self.limits.get_mut(&action).unwrap();
        if limit.check_limited(ip) {
            return Err(APIError::Ratelimited())
        }
        limit.add_limit(ip, action.get_limit());
        Ok(())
    }
}

pub struct Limits {
    blocked_times: Vec<DateTime<Utc>>,
    searchable_users: LinkedHashSet<UniqueIdentifier>,
}

#[derive(Hash, Ord, PartialOrd, Eq, PartialEq, Clone, Copy, Debug)]
pub enum UniqueIdentifier {
    Ipv4(Ipv4Addr),
    Ipv6(Ipv6Addr),
    Discord(u64),
}

impl Limits {
    pub fn check_limited(&mut self, ip: &UniqueIdentifier) -> bool {
        while !self.blocked_times.is_empty() {
            if self.blocked_times[0] < DateTime::<Utc>::from(SystemTime::now()) {
                self.blocked_times.pop();
                self.searchable_users.pop_front();
            } else {
                break;
            }
        }

        self.searchable_users.contains(ip)
    }

    pub fn add_limit(&mut self, ip: &UniqueIdentifier, time: f64) {
        self.blocked_times.insert(
            self.blocked_times.len(),
            DateTime::from(SystemTime::now()).add(TimeDelta::new(time as i64, (time.fract() * 1000000000.0) as u32).unwrap()),
        );
        self.searchable_users.insert(ip.clone());
    }
}

pub const ACTIONS: [SiteAction; 5] = [
    SiteAction::Search,
    SiteAction::Upload,
    SiteAction::Update,
    SiteAction::UpvoteList,
    SiteAction::Download,
];

#[derive(Eq, PartialEq, Hash, Ord, PartialOrd, Copy, Clone)]
pub enum SiteAction {
    Search,
    Download,
    Update,
    Upload,
    UpvoteList,
}

impl SiteAction {
    // MUST UPDATE ACTIONS AS WELL
    pub fn get_limit(&self) -> f64 {
        match self {
            SiteAction::Search | SiteAction::Download | SiteAction::UpvoteList => 0.25,
            SiteAction::Update => 60.0,
            SiteAction::Upload => 60.0 * 60.0 * 12.0,
        }
    }
}
