use std::ops::Deref;

use super::request::{AccessRequest, Gating};

#[derive(Debug)]
#[must_use]
pub struct Clearance {
    _sealed: (),
}

impl Clearance {
    pub(super) const fn new() -> Self {
        Self { _sealed: () }
    }

    pub fn open(request: &AccessRequest) -> Option<Self> {
        (request.gating == Gating::Open && request.grant_resources().is_empty()).then(Self::new)
    }

    pub const fn admit<P>(self, prepared: P) -> Admitted<P> {
        Admitted { prepared }
    }
}

#[derive(Debug)]
pub struct Admitted<P> {
    prepared: P,
}

#[cfg(test)]
pub const fn admitted<P>(prepared: P) -> Admitted<P> {
    Admitted { prepared }
}

impl<P> Admitted<P> {
    pub fn into_inner(self) -> P {
        self.prepared
    }

    pub const fn get(&self) -> &P {
        &self.prepared
    }
}

impl<P> Deref for Admitted<P> {
    type Target = P;

    fn deref(&self) -> &P {
        &self.prepared
    }
}
