pub mod setting;

use rusqlite::Connection;

pub use setting::SettingRepository;

pub struct Repositories<'c> {
    pub settings: SettingRepository<'c>,
}

impl<'c> Repositories<'c> {
    pub const fn new(connection: &'c Connection) -> Self {
        Self {
            settings: SettingRepository::new(connection),
        }
    }
}
