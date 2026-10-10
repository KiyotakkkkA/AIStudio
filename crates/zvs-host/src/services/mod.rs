pub mod system;

pub use system::SystemService;

pub struct Services {
    pub system: SystemService,
}
