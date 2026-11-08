#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod catalog;
mod core;
mod install;
mod state;
mod ui;
mod updater;

use crate::core::{Core, Settings, TitleView};
use iced::futures::channel::mpsc;
use iced::futures::SinkExt;
use iced::widget::image;
use iced::{Subscription, Task};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

fn main() -> iced::Result {
    let icon = iced::window::icon::from_file_data(include_bytes!("../assets/window-icon.png"), None).ok();
    iced::application(App::boot, App::update, App::view)
        .title("Rosace Launcher")
        .theme(|_: &App| iced::Theme::Dark)
        .subscription(App::subscription)
        .window(iced::window::Settings {
            size: iced::Size::new(1100.0, 680.0),
            min_size: Some(iced::Size::new(820.0, 520.0)),
            icon,
            ..Default::default()
        })
        .run()
}

#[derive(Clone, Debug)]
pub enum Msg {
    Loaded(Result<Vec<TitleView>, String>),
    Release(String, Result<install::ReleaseInfo, String>),
    Cover(String, Option<image::Handle>),
    Select(String),
    Install(String),
    Launch(String),
    Folder(String),
    AskUninstall(String),
    CancelUninstall,
    Uninstall(String),
    Store(String),
    Progress(install::Progress),
    Installed(String, Result<state::Installed, String>),
    Done(String, Result<(), String>),
    AutoUpdate(bool),
    Refresh,
    Tick,
    Kofi,
    Ignore,
    LauncherUpdate(Result<Option<updater::Update>, String>),
    DoLauncherUpdate,
    LauncherUpdateFailed(String),
}

pub struct App {
    pub core: Core,
    pub settings: Settings,
    pub titles: Vec<TitleView>,
    pub covers: HashMap<String, image::Handle>,
    pub releases: HashMap<String, Result<install::ReleaseInfo, String>>,
    pub progress: HashMap<String, install::Progress>,
    pub errors: HashMap<String, String>,
    pub selected: Option<String>,
    pub confirm_uninstall: Option<String>,
    pub load_error: Option<String>,
    pub loading: bool,
    /// Titles to start as soon as their update finished installing ("never start an outdated build").
    pub launch_after: HashSet<String>,
    pub launcher_update: Option<updater::Update>,
    pub launcher_updating: bool,
    pub launcher_update_error: Option<String>,
}

impl App {
    fn boot() -> (Self, Task<Msg>) {
        let core = Core::new();
        let app = App {
            settings: core::load_settings(&core.data_dir),
            core,
            titles: Vec::new(),
            covers: HashMap::new(),
            releases: HashMap::new(),
            progress: HashMap::new(),
            errors: HashMap::new(),
            selected: None,
            confirm_uninstall: None,
            load_error: None,
            loading: true,
            launch_after: HashSet::new(),
            launcher_update: None,
            launcher_updating: false,
            launcher_update_error: None,
        };
        let task = Task::batch([app.load_task(), app.check_launcher_task()]);
        (app, task)
    }

    fn load_task(&self) -> Task<Msg> {
        let core = self.core.clone();
        Task::perform(async move { core.list_titles().await }, Msg::Loaded)
    }

    fn check_launcher_task(&self) -> Task<Msg> {
        let client = self.core.client.clone();
        Task::perform(async move { updater::check(&client).await }, Msg::LauncherUpdate)
    }

    fn release_task(&self, id: &str) -> Task<Msg> {
        let core = self.core.clone();
        let id = id.to_string();
        Task::perform(
            async move {
                let r = core.latest_release(&id).await;
                (id, r)
            },
            |(id, r)| Msg::Release(id, r),
        )
    }

    fn install_task(&mut self, id: &str) -> Task<Msg> {
        if self.progress.contains_key(id) {
            return Task::none();
        }
        let total = self.releases.get(id).and_then(|r| r.as_ref().ok()).map(|r| r.size).unwrap_or(0);
        self.progress.insert(id.to_string(), install::Progress { id: id.to_string(), stage: "download", downloaded: 0, total });
        self.errors.remove(id);
        let core = self.core.clone();
        let id = id.to_string();
        Task::stream(iced::stream::channel(64, move |mut out: mpsc::Sender<Msg>| async move {
            let tx = out.clone();
            let r = core
                .install_title(&id, &move |p| {
                    let _ = tx.clone().try_send(Msg::Progress(p));
                })
                .await;
            let _ = out.send(Msg::Installed(id, r)).await;
        }))
    }

    fn status(&self, t: &TitleView) -> ui::Status {
        ui::status(self, t)
    }

    fn update(&mut self, msg: Msg) -> Task<Msg> {
        match msg {
            Msg::Loaded(Ok(titles)) => {
                self.loading = false;
                self.load_error = None;
                self.titles = titles;
                if !self.titles.iter().any(|t| Some(&t.title.id) == self.selected.as_ref()) {
                    self.selected = self.titles.first().map(|t| t.title.id.clone());
                }
                self.releases.clear();
                let mut tasks = Vec::new();
                for t in &self.titles {
                    if t.supported && t.title.store_url.is_none() {
                        tasks.push(self.release_task(&t.title.id));
                    }
                    if let Some(c) = &t.title.cover {
                        if let Some(h) = ui::embedded_cover(c) {
                            self.covers.insert(t.title.id.clone(), h);
                        } else if c.starts_with("https://") {
                            let (client, id, url) = (self.core.client.clone(), t.title.id.clone(), c.clone());
                            tasks.push(Task::perform(
                                async move {
                                    let bytes = client.get(&url).send().await.ok()?.bytes().await.ok()?;
                                    Some(image::Handle::from_bytes(bytes.to_vec()))
                                },
                                move |h| Msg::Cover(id.clone(), h),
                            ));
                        }
                    }
                }
                Task::batch(tasks)
            }
            Msg::Loaded(Err(e)) => {
                self.loading = false;
                self.load_error = Some(e);
                Task::none()
            }
            Msg::Release(id, r) => {
                self.releases.insert(id.clone(), r);
                // Installed titles update themselves when a newer release is found.
                let outdated = self.titles.iter().find(|t| t.title.id == id).map(|t| self.status(t) == ui::Status::Update).unwrap_or(false);
                if outdated && self.settings.auto_update {
                    return self.install_task(&id);
                }
                Task::none()
            }
            Msg::Cover(id, Some(h)) => {
                self.covers.insert(id, h);
                Task::none()
            }
            Msg::Cover(_, None) | Msg::Ignore => Task::none(),
            Msg::Select(id) => {
                self.selected = Some(id);
                self.confirm_uninstall = None;
                Task::none()
            }
            Msg::Install(id) => self.install_task(&id),
            Msg::Launch(id) => {
                let outdated = self.titles.iter().find(|t| t.title.id == id).map(|t| self.status(t) == ui::Status::Update).unwrap_or(false);
                if outdated {
                    self.launch_after.insert(id.clone());
                    return self.install_task(&id);
                }
                self.errors.remove(&id);
                let core = self.core.clone();
                Task::perform(
                    async move {
                        let r = core.launch_title(&id).await;
                        (id, r)
                    },
                    |(id, r)| Msg::Done(id, r),
                )
            }
            Msg::Folder(id) => {
                let r = self.core.open_install_folder(&id);
                Task::done(Msg::Done(id, r))
            }
            Msg::AskUninstall(id) => {
                self.confirm_uninstall = Some(id);
                Task::none()
            }
            Msg::CancelUninstall => {
                self.confirm_uninstall = None;
                Task::none()
            }
            Msg::Uninstall(id) => {
                self.confirm_uninstall = None;
                let r = self.core.uninstall_title(&id);
                if r.is_ok() {
                    if let Some(t) = self.titles.iter_mut().find(|t| t.title.id == id) {
                        t.installed = None;
                    }
                }
                Task::done(Msg::Done(id, r))
            }
            Msg::Store(url) => {
                let core = self.core.clone();
                Task::perform(async move { core.open_link(&url).await }, |_| Msg::Ignore)
            }
            Msg::Kofi => {
                let core = self.core.clone();
                Task::perform(async move { core.open_link(core::KOFI_URL).await }, |_| Msg::Ignore)
            }
            Msg::Progress(p) => {
                if p.stage != "done" && self.progress.contains_key(&p.id) {
                    self.progress.insert(p.id.clone(), p);
                }
                Task::none()
            }
            Msg::Installed(id, r) => {
                self.progress.remove(&id);
                let launch = self.launch_after.remove(&id);
                match r {
                    Ok(installed) => {
                        if let Some(t) = self.titles.iter_mut().find(|t| t.title.id == id) {
                            t.installed = Some(installed);
                        }
                        if launch {
                            return Task::done(Msg::Launch(id));
                        }
                    }
                    Err(e) => {
                        self.errors.insert(id, e);
                    }
                }
                Task::none()
            }
            Msg::Done(id, r) => {
                match r {
                    Ok(()) => {
                        self.errors.remove(&id);
                    }
                    Err(e) => {
                        self.errors.insert(id, e);
                    }
                }
                Task::none()
            }
            Msg::AutoUpdate(on) => {
                self.settings.auto_update = on;
                core::save_settings(&self.core.data_dir, &self.settings);
                Task::none()
            }
            Msg::Refresh => {
                self.loading = true;
                self.load_task()
            }
            Msg::Tick => {
                if self.progress.is_empty() {
                    Task::batch([self.load_task(), self.check_launcher_task()])
                } else {
                    Task::none()
                }
            }
            Msg::LauncherUpdate(r) => {
                self.launcher_update = r.ok().flatten();
                Task::none()
            }
            Msg::DoLauncherUpdate => {
                let Some(u) = self.launcher_update.clone() else { return Task::none() };
                self.launcher_updating = true;
                self.launcher_update_error = None;
                let (client, dir) = (self.core.client.clone(), self.core.data_dir.clone());
                Task::perform(
                    async move {
                        match updater::install(&client, &dir, &u).await {
                            Ok(()) => String::new(),
                            Err(e) => e,
                        }
                    },
                    Msg::LauncherUpdateFailed,
                )
            }
            Msg::LauncherUpdateFailed(e) => {
                self.launcher_updating = false;
                self.launcher_update_error = Some(e);
                Task::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Msg> {
        // Check for new releases every 30 minutes while the launcher stays open.
        iced::time::every(Duration::from_secs(30 * 60)).map(|_| Msg::Tick)
    }

    fn view(&self) -> iced::Element<'_, Msg> {
        ui::view(self)
    }
}
