use std::collections::HashMap;

use zvs_contract::{BrowserBounds, BrowserSecurity, BrowserState, BrowserTab};
use zvs_host::browser::{
    CHROME_HEIGHT, NAVIGATION_BLOCKED, STATUS_HEIGHT, TAB_LIMIT, devtools_height,
};

pub const NEW_TAB_TITLE: &str = "Новая вкладка";
pub const PROCESS_GONE: &str = "Page process stopped. Reload to retry.";
pub const FORM_RESUBMISSION: &str =
    "This page needs a form submission. Go back to the sign-in page and try again.";
pub const TAB_LIMIT_REACHED: &str = "Close a tab before opening another (limit: 30)";
pub const UNKNOWN_TAB: &str = "Unknown tab";
pub const INVALID_ORDER: &str = "Invalid tab order";
pub const NO_ACTIVE_TAB: &str = "No active tab";

const BLANK: &str = "about:blank";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PageSnapshot {
    pub url: String,
    pub title: String,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadFailure {
    Aborted,
    CacheMiss,
    Other(i32),
}

pub fn load_error(failure: LoadFailure) -> Option<String> {
    match failure {
        LoadFailure::Aborted => None,
        LoadFailure::CacheMiss => Some(FORM_RESUBMISSION.to_owned()),
        LoadFailure::Other(code) => Some(format!("Page could not load ({code})")),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageSignal {
    Started,
    Snapshot(PageSnapshot),
    Finished(Option<LoadFailure>),
    Interactive,
    Blocked,
    Crashed,
    CloseRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tab {
    pub id: String,
    pub label: String,
    pub page: PageSnapshot,
    pub loading: bool,
    pub error: Option<String>,
}

impl Tab {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            page: PageSnapshot::default(),
            loading: false,
            error: None,
        }
    }

    fn state(&self) -> BrowserTab {
        let url = self.page.url.clone();
        let security = if self.error.is_some() {
            BrowserSecurity::Error
        } else if self.loading {
            BrowserSecurity::None
        } else if url.starts_with("https:") {
            BrowserSecurity::Https
        } else if url.starts_with("http:") {
            BrowserSecurity::Http
        } else {
            BrowserSecurity::None
        };
        BrowserTab {
            id: self.id.clone(),
            title: if self.page.title.is_empty() {
                NEW_TAB_TITLE.to_owned()
            } else {
                self.page.title.clone()
            },
            url,
            loading: self.loading,
            can_go_back: self.page.can_go_back,
            can_go_forward: self.page.can_go_forward,
            security,
            error: self.error.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub visible: bool,
    pub chrome: Rect,
    pub site: Option<(String, Rect)>,
}

#[derive(Debug, Default)]
pub struct Model {
    tabs: Vec<Tab>,
    active: Option<String>,
    sites_visible: bool,
    devtools_visible: bool,
    visible: bool,
    bounds: BrowserBounds,
    zoom: Option<f64>,
}

impl Model {
    pub fn set_zoom(&mut self, factor: f64) -> bool {
        let next = (factor.is_finite() && factor > 0.0).then_some(factor);
        let changed = next != self.zoom;
        self.zoom = next;
        changed
    }

    pub fn zoom(&self) -> f64 {
        self.zoom.unwrap_or(1.0)
    }

    pub fn mount(&mut self, bounds: BrowserBounds) {
        self.bounds = bounds;
        self.visible = true;
    }

    pub const fn hide(&mut self) {
        self.visible = false;
    }

    pub fn has_room(&self) -> bool {
        self.tabs.len() < TAB_LIMIT
    }

    pub fn open(&mut self, tab: Tab) -> Result<(), &'static str> {
        if !self.has_room() {
            return Err(TAB_LIMIT_REACHED);
        }
        self.active = Some(tab.id.clone());
        self.sites_visible = false;
        self.tabs.push(tab);
        Ok(())
    }

    pub fn active(&self) -> Option<&Tab> {
        let active = self.active.as_deref()?;
        self.tabs.iter().find(|tab| tab.id == active)
    }

    pub fn switch(&mut self, id: &str) -> Result<(), &'static str> {
        if !self.tabs.iter().any(|tab| tab.id == id) {
            return Err(UNKNOWN_TAB);
        }
        self.active = Some(id.to_owned());
        self.sites_visible = false;
        Ok(())
    }

    pub fn close(&mut self, id: &str) -> Result<Tab, &'static str> {
        let index = self
            .tabs
            .iter()
            .position(|tab| tab.id == id)
            .ok_or(UNKNOWN_TAB)?;
        let removed = self.tabs.remove(index);
        if self.active.as_deref() == Some(id) {
            self.active = self
                .tabs
                .get(index.min(self.tabs.len().saturating_sub(1)))
                .map(|tab| tab.id.clone());
        }
        Ok(removed)
    }

    pub fn forget(&mut self, label: &str) -> Option<Tab> {
        let index = self.tabs.iter().position(|tab| tab.label == label)?;
        let removed = self.tabs.remove(index);
        if self.active.as_deref() == Some(removed.id.as_str()) {
            self.active = self
                .tabs
                .get(index.saturating_sub(1))
                .map(|tab| tab.id.clone());
        }
        Some(removed)
    }

    pub fn reorder(&mut self, id: &str, index: u64) -> Result<(), &'static str> {
        let from = self.tabs.iter().position(|tab| tab.id == id);
        let to = usize::try_from(index)
            .ok()
            .filter(|to| *to < self.tabs.len());
        let (Some(from), Some(to)) = (from, to) else {
            return Err(INVALID_ORDER);
        };
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        Ok(())
    }

    pub const fn set_sites_visible(&mut self, visible: bool) {
        self.sites_visible = visible;
    }

    pub const fn set_devtools_visible(&mut self, visible: bool) {
        self.devtools_visible = visible;
    }

    pub fn label_of(&self, id: &str) -> Option<String> {
        self.tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.label.clone())
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn clear(&mut self) -> Vec<Tab> {
        self.active = None;
        self.sites_visible = false;
        self.devtools_visible = false;
        self.visible = false;
        std::mem::take(&mut self.tabs)
    }

    pub fn apply(&mut self, label: &str, signal: PageSignal) -> bool {
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.label == label) else {
            return false;
        };
        match signal {
            PageSignal::Started => {
                tab.error = None;
                tab.loading = true;
            }
            PageSignal::Snapshot(mut page) => {
                if page.url == BLANK {
                    page.url.clear();
                }
                tab.page = page;
            }
            PageSignal::Finished(failure) => {
                tab.loading = false;
                if let Some(message) = failure.and_then(load_error) {
                    tab.error = Some(message);
                }
            }
            PageSignal::Blocked => tab.error = Some(NAVIGATION_BLOCKED.to_owned()),
            PageSignal::Crashed => tab.error = Some(PROCESS_GONE.to_owned()),
            PageSignal::CloseRequested | PageSignal::Interactive => return false,
        }
        true
    }

    pub fn state(&self) -> BrowserState {
        BrowserState {
            tabs: self.tabs.iter().map(Tab::state).collect(),
            active_id: self.active.clone(),
            sites_visible: self.sites_visible,
            devtools_visible: self.devtools_visible,
        }
    }

    pub fn layout(&self, window_width: f64, window_height: f64) -> Layout {
        let x = self.bounds.x.round().min(window_width);
        let y = self.bounds.y.round().min(window_height);
        let width = self.bounds.width.round().min(window_width - x);
        let height = self.bounds.height.round().min(window_height - y);
        let chrome = Rect {
            x,
            y,
            width,
            height,
        };
        let site = self
            .active()
            .filter(|tab| self.visible && !self.sites_visible && !tab.page.url.is_empty())
            .map(|tab| {
                let chrome_height = CHROME_HEIGHT * self.zoom();
                let content = (height - chrome_height - STATUS_HEIGHT * self.zoom()).max(0.0);
                let devtools = if self.devtools_visible {
                    devtools_height(content)
                } else {
                    0.0
                };
                (
                    tab.label.clone(),
                    Rect {
                        x,
                        y: y + chrome_height,
                        width,
                        height: content - devtools,
                    },
                )
            });
        Layout {
            visible: self.visible,
            chrome,
            site,
        }
    }
}

#[derive(Debug, Default)]
pub struct LinkTabs(HashMap<String, String>);

impl LinkTabs {
    pub fn opened(&mut self, label: impl Into<String>, url: impl Into<String>) {
        self.0.insert(label.into(), url.into());
    }

    pub fn closed(&mut self, label: &str) -> Option<String> {
        self.0.remove(label)
    }

    pub fn finished(&mut self, url: &str) {
        self.0.retain(|_, linked| linked != url);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_link_tab_is_reported_once_when_it_closes_and_forgotten_when_linking_finishes() {
        let mut links = LinkTabs::default();
        links.opened("browser-site-1", "https://chat.qwen.ai/");
        links.opened("browser-site-2", "https://chat.qwen.ai/c/second");
        assert_eq!(links.closed("browser-site-9"), None);
        assert_eq!(
            links.closed("browser-site-1").as_deref(),
            Some("https://chat.qwen.ai/")
        );
        assert_eq!(links.closed("browser-site-1"), None);
        links.finished("https://chat.qwen.ai/c/second");
        assert_eq!(links.closed("browser-site-2"), None);
        links.opened("browser-site-3", "https://chat.qwen.ai/");
        links.clear();
        assert_eq!(links.closed("browser-site-3"), None);
    }

    const A: &str = "0199aa11-1111-7111-8111-00000000000a";
    const B: &str = "0199aa11-1111-7111-8111-00000000000b";
    const C: &str = "0199aa11-1111-7111-8111-00000000000c";

    fn page(url: &str, title: &str) -> PageSnapshot {
        PageSnapshot {
            url: url.to_owned(),
            title: title.to_owned(),
            can_go_back: false,
            can_go_forward: false,
        }
    }

    fn three() -> Model {
        let mut model = Model::default();
        for (id, label) in [(A, "site-a"), (B, "site-b"), (C, "site-c")] {
            model.open(Tab::new(id, label)).expect("room");
        }
        model
    }

    fn order(model: &Model) -> Vec<String> {
        model.state().tabs.into_iter().map(|tab| tab.id).collect()
    }

    #[test]
    fn a_fresh_tab_is_active_untitled_and_insecure() {
        let mut model = Model::default();
        model.open(Tab::new(A, "site-a")).expect("room");
        model.set_sites_visible(true);
        model.open(Tab::new(B, "site-b")).expect("room");
        let state = model.state();
        assert_eq!(state.active_id.as_deref(), Some(B));
        assert!(!state.sites_visible);
        assert_eq!(
            state.tabs[1],
            BrowserTab {
                id: B.into(),
                title: NEW_TAB_TITLE.into(),
                url: String::new(),
                loading: false,
                can_go_back: false,
                can_go_forward: false,
                security: BrowserSecurity::None,
                error: None,
            }
        );
    }

    #[test]
    fn security_follows_the_error_loading_and_scheme() {
        let mut model = three();
        model.apply(
            "site-a",
            PageSignal::Snapshot(page("https://a.example/", "A")),
        );
        model.apply(
            "site-b",
            PageSignal::Snapshot(page("http://b.example/", "B")),
        );
        model.apply(
            "site-c",
            PageSignal::Snapshot(page("https://c.example/", "C")),
        );
        model.apply("site-c", PageSignal::Started);
        let security: Vec<BrowserSecurity> =
            model.state().tabs.iter().map(|tab| tab.security).collect();
        assert_eq!(
            security,
            [
                BrowserSecurity::Https,
                BrowserSecurity::Http,
                BrowserSecurity::None
            ]
        );
        model.apply(
            "site-a",
            PageSignal::Finished(Some(LoadFailure::Other(-105))),
        );
        let tab = &model.state().tabs[0];
        assert_eq!(tab.security, BrowserSecurity::Error);
        assert_eq!(tab.error.as_deref(), Some("Page could not load (-105)"));
    }

    #[test]
    fn load_failures_map_to_messages() {
        assert_eq!(load_error(LoadFailure::Aborted), None);
        assert_eq!(
            load_error(LoadFailure::CacheMiss).as_deref(),
            Some(FORM_RESUBMISSION)
        );
        assert_eq!(
            load_error(LoadFailure::Other(7)).as_deref(),
            Some("Page could not load (7)")
        );
    }

    #[test]
    fn a_new_main_navigation_clears_the_error_and_an_abort_keeps_it() {
        let mut model = three();
        model.apply("site-a", PageSignal::Blocked);
        model.apply("site-a", PageSignal::Finished(Some(LoadFailure::Aborted)));
        assert_eq!(
            model.state().tabs[0].error.as_deref(),
            Some(NAVIGATION_BLOCKED)
        );
        model.apply("site-a", PageSignal::Started);
        assert_eq!(model.state().tabs[0].error, None);
        assert!(model.state().tabs[0].loading);
        model.apply("site-a", PageSignal::Crashed);
        assert_eq!(model.state().tabs[0].error.as_deref(), Some(PROCESS_GONE));
    }

    #[test]
    fn a_blank_page_reads_as_an_empty_address() {
        let mut model = three();
        model.apply("site-a", PageSignal::Snapshot(page("about:blank", "")));
        assert_eq!(model.state().tabs[0].url, "");
        assert!(!model.apply("unknown", PageSignal::Started));
    }

    #[test]
    fn closing_the_active_tab_selects_the_one_that_takes_its_place() {
        let mut model = three();
        model.switch(B).expect("known");
        model.close(B).expect("known");
        assert_eq!(model.state().active_id.as_deref(), Some(C));
        model.close(C).expect("known");
        assert_eq!(model.state().active_id.as_deref(), Some(A));
        model.close(A).expect("known");
        assert_eq!(model.state().active_id, None);
        assert!(model.is_empty());
        assert_eq!(model.close(A), Err(UNKNOWN_TAB));
    }

    #[test]
    fn closing_an_inactive_tab_keeps_the_selection() {
        let mut model = three();
        model.close(A).expect("known");
        assert_eq!(model.state().active_id.as_deref(), Some(C));
    }

    #[test]
    fn a_destroyed_tab_hands_the_selection_to_its_left_neighbour() {
        let mut model = three();
        model.switch(B).expect("known");
        assert_eq!(model.forget("site-b").map(|tab| tab.id), Some(B.to_owned()));
        assert_eq!(model.state().active_id.as_deref(), Some(A));
        model.forget("site-a");
        assert_eq!(model.state().active_id.as_deref(), Some(C));
        assert_eq!(model.forget("site-a"), None);
    }

    #[test]
    fn reorder_moves_a_tab_and_rejects_out_of_range_targets() {
        let mut model = three();
        model.reorder(C, 0).expect("valid");
        assert_eq!(order(&model), [C, A, B]);
        model.reorder(C, 2).expect("valid");
        assert_eq!(order(&model), [A, B, C]);
        assert_eq!(model.reorder(C, 3), Err(INVALID_ORDER));
        assert_eq!(
            model.reorder("0199aa11-1111-7111-8111-00000000000d", 0),
            Err(INVALID_ORDER)
        );
    }

    #[test]
    fn switching_to_an_unknown_tab_fails() {
        let mut model = three();
        model.set_sites_visible(true);
        assert_eq!(model.switch("nope"), Err(UNKNOWN_TAB));
        assert!(model.state().sites_visible);
        model.switch(A).expect("known");
        assert!(!model.state().sites_visible);
    }

    #[test]
    fn at_most_thirty_tabs_open() {
        let mut model = Model::default();
        for index in 0..TAB_LIMIT {
            model
                .open(Tab::new(index.to_string(), format!("site-{index}")))
                .expect("room");
        }
        assert!(!model.has_room());
        assert_eq!(model.open(Tab::new("x", "site-x")), Err(TAB_LIMIT_REACHED));
    }

    #[test]
    fn the_page_sits_below_the_chrome_and_above_the_status_bar() {
        let mut model = three();
        model.mount(BrowserBounds {
            x: 245.0,
            y: 9.0,
            width: 1186.0,
            height: 862.0,
        });
        model.apply(
            "site-c",
            PageSignal::Snapshot(page("https://c.example/", "C")),
        );
        let layout = model.layout(1440.0, 900.0);
        assert!(layout.visible);
        assert_eq!(
            layout.chrome,
            Rect {
                x: 245.0,
                y: 9.0,
                width: 1186.0,
                height: 862.0
            }
        );
        assert_eq!(
            layout.site,
            Some((
                "site-c".to_owned(),
                Rect {
                    x: 245.0,
                    y: 157.0,
                    width: 1186.0,
                    height: 686.0
                }
            ))
        );
    }

    #[test]
    fn the_chrome_and_status_bar_scale_with_the_interface_zoom() {
        let mut model = three();
        model.mount(BrowserBounds {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        });
        model.apply(
            "site-c",
            PageSignal::Snapshot(page("https://c.example/", "C")),
        );
        assert!(model.set_zoom(1.25));
        assert!(!model.set_zoom(1.25));
        let site = |model: &Model| model.layout(800.0, 600.0).site.map(|(_, rect)| rect);
        assert_eq!(
            site(&model),
            Some(Rect {
                x: 0.0,
                y: 185.0,
                width: 800.0,
                height: 380.0
            })
        );
        model.set_devtools_visible(true);
        assert_eq!(site(&model).map(|rect| rect.height), Some(209.0));
        assert!(model.set_zoom(f64::NAN));
        assert!((model.zoom() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn layout_is_clamped_to_the_window() {
        let mut model = three();
        model.mount(BrowserBounds {
            x: 900.4,
            y: 2000.0,
            width: 500.0,
            height: 100.0,
        });
        let layout = model.layout(1000.0, 800.0);
        assert_eq!(
            layout.chrome,
            Rect {
                x: 900.0,
                y: 800.0,
                width: 100.0,
                height: 0.0
            }
        );
    }

    #[test]
    fn no_page_is_shown_while_hidden_blank_or_on_sites() {
        let mut model = three();
        model.mount(BrowserBounds {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        });
        assert_eq!(model.layout(800.0, 600.0).site, None);
        model.apply(
            "site-c",
            PageSignal::Snapshot(page("https://c.example/", "C")),
        );
        assert!(model.layout(800.0, 600.0).site.is_some());
        model.set_sites_visible(true);
        assert_eq!(model.layout(800.0, 600.0).site, None);
        model.set_sites_visible(false);
        model.set_devtools_visible(true);
        assert_eq!(
            model.layout(800.0, 600.0).site.map(|(_, rect)| rect.height),
            Some(233.0)
        );
        assert!(model.state().devtools_visible);
        model.hide();
        let hidden = model.layout(800.0, 600.0);
        assert!(!hidden.visible);
        assert_eq!(hidden.site, None);
        assert_eq!(model.state().tabs.len(), 3);
    }

    #[test]
    fn clearing_drops_every_tab() {
        let mut model = three();
        model.set_devtools_visible(true);
        assert_eq!(model.label_of(A).as_deref(), Some("site-a"));
        assert_eq!(model.clear().len(), 3);
        assert_eq!(model.state(), BrowserState::default());
    }
}
