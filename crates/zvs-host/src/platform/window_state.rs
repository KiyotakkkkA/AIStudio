use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const WINDOW_STATE_KEY: &str = "window.main";
pub const MIN_WINDOW_WIDTH: i64 = 1100;
pub const MIN_WINDOW_HEIGHT: i64 = 700;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<i64>,
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub bounds: WindowBounds,
    pub maximized: bool,
}

pub const DEFAULT_WINDOW_STATE: WindowState = WindowState {
    bounds: WindowBounds {
        x: None,
        y: None,
        width: 1440,
        height: 900,
    },
    maximized: true,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NormalBounds {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub width: f64,
    pub height: f64,
}

impl From<WindowBounds> for NormalBounds {
    fn from(bounds: WindowBounds) -> Self {
        Self {
            x: bounds.x.map(|x| x as f64),
            y: bounds.y.map(|y| y as f64),
            width: bounds.width as f64,
            height: bounds.height as f64,
        }
    }
}

pub trait WindowLike {
    fn is_maximized(&self) -> bool;
    fn is_full_screen(&self) -> bool;
    fn normal_bounds(&self) -> NormalBounds;
}

fn finite(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|number| number.is_finite())
}

fn rounded(value: f64) -> i64 {
    value.round() as i64
}

pub fn read_window_state(stored: Option<&Value>) -> WindowState {
    let Some(Value::Object(stored)) = stored else {
        return DEFAULT_WINDOW_STATE;
    };
    let Some(Value::Object(bounds)) = stored.get("bounds") else {
        return DEFAULT_WINDOW_STATE;
    };
    let (Some(width), Some(height)) = (finite(bounds.get("width")), finite(bounds.get("height")))
    else {
        return DEFAULT_WINDOW_STATE;
    };
    let position = finite(bounds.get("x")).zip(finite(bounds.get("y")));
    WindowState {
        bounds: WindowBounds {
            x: position.map(|(x, _)| rounded(x)),
            y: position.map(|(_, y)| rounded(y)),
            width: rounded(width).max(MIN_WINDOW_WIDTH),
            height: rounded(height).max(MIN_WINDOW_HEIGHT),
        },
        maximized: stored.get("maximized") == Some(&Value::Bool(true)),
    }
}

pub fn capture_window_state(window: &impl WindowLike) -> WindowState {
    let bounds = window.normal_bounds();
    let position = bounds.x.zip(bounds.y);
    WindowState {
        bounds: WindowBounds {
            x: position.map(|(x, _)| rounded(x)),
            y: position.map(|(_, y)| rounded(y)),
            width: rounded(bounds.width),
            height: rounded(bounds.height),
        },
        maximized: window.is_maximized() || window.is_full_screen(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    struct Window {
        maximized: bool,
        full_screen: bool,
    }

    impl WindowLike for Window {
        fn is_maximized(&self) -> bool {
            self.maximized
        }

        fn is_full_screen(&self) -> bool {
            self.full_screen
        }

        fn normal_bounds(&self) -> NormalBounds {
            NormalBounds {
                x: Some(100.0),
                y: Some(60.0),
                width: 1600.0,
                height: 1000.0,
            }
        }
    }

    #[test]
    fn captured_geometry_reads_back_unchanged() {
        let captured = capture_window_state(&Window {
            maximized: false,
            full_screen: false,
        });
        let stored = serde_json::to_value(captured).expect("serialises");
        assert_eq!(
            stored,
            json!({ "bounds": { "x": 100, "y": 60, "width": 1600, "height": 1000 }, "maximized": false })
        );
        assert_eq!(read_window_state(Some(&stored)), captured);
    }

    #[test]
    fn full_screen_counts_as_maximised() {
        let captured = capture_window_state(&Window {
            maximized: false,
            full_screen: true,
        });
        assert!(captured.maximized);
    }

    #[test]
    fn unusable_stored_state_falls_back_to_the_default() {
        assert_eq!(read_window_state(None), DEFAULT_WINDOW_STATE);
        assert_eq!(
            read_window_state(Some(&json!("wide"))),
            DEFAULT_WINDOW_STATE
        );
        assert_eq!(
            read_window_state(Some(&json!({ "bounds": { "width": "wide" } }))),
            DEFAULT_WINDOW_STATE
        );
        assert_eq!(
            read_window_state(Some(&json!({ "bounds": null }))),
            DEFAULT_WINDOW_STATE
        );
    }

    #[test]
    fn small_windows_are_clamped_and_a_partial_position_is_dropped() {
        assert_eq!(
            read_window_state(Some(
                &json!({ "bounds": { "width": 10, "height": 10 }, "maximized": true })
            )),
            WindowState {
                bounds: WindowBounds {
                    x: None,
                    y: None,
                    width: 1100,
                    height: 700,
                },
                maximized: true,
            }
        );
        assert_eq!(
            read_window_state(Some(
                &json!({ "bounds": { "x": 5, "width": 1200.4, "height": 800.6 }, "maximized": "yes" })
            )),
            WindowState {
                bounds: WindowBounds {
                    x: None,
                    y: None,
                    width: 1200,
                    height: 801,
                },
                maximized: false,
            }
        );
    }

    #[test]
    fn a_window_that_never_left_maximised_keeps_its_stored_size_without_a_position() {
        struct Maximised;
        impl WindowLike for Maximised {
            fn is_maximized(&self) -> bool {
                true
            }
            fn is_full_screen(&self) -> bool {
                false
            }
            fn normal_bounds(&self) -> NormalBounds {
                DEFAULT_WINDOW_STATE.bounds.into()
            }
        }
        assert_eq!(capture_window_state(&Maximised), DEFAULT_WINDOW_STATE);
    }
}
