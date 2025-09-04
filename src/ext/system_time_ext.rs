use std::time::SystemTime;

use time::Time;

pub trait SystemTimeExt {
    fn to_time(&self) -> Time;
}

impl SystemTimeExt for SystemTime {
    fn to_time(&self) -> Time {
        self.duration_since(SystemTime::UNIX_EPOCH)
            .ok()
            .map(|d| {
                Time::from_hms_nano(
                    (d.as_secs() / 3600 % 24) as u8,
                    (d.as_secs() / 60 % 60) as u8,
                    (d.as_secs() % 60) as u8,
                    d.subsec_nanos(),
                )
                .unwrap_or_else(|_| Time::MIDNIGHT)
            })
            .unwrap_or_else(|| Time::MIDNIGHT)
    }
}
