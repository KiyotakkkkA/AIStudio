use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::validate::{Validate, check};

pub const PROFILE_AVATAR_MAX: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileAvatarDto {
    pub svg: String,
}

impl Validate for ProfileAvatarDto {
    fn validate(&self) -> Result<(), AppError> {
        check::non_empty("svg", &self.svg)?;
        check::max_len("svg", &self.svg, PROFILE_AVATAR_MAX)
    }
}
