pub fn hide_from_capture(_raw: isize) -> bool {
    false
}

pub fn set_no_activate(_raw: isize, _enabled: bool) {}

pub fn show_inactive(_raw: isize) {}

pub fn watch_stop_key(_trigger: impl Fn() + Send + 'static) {}

pub fn release_stop_key() {}
