//! The Iced view: black and white theme, sidebar of titles, detail pane.

use crate::core::TitleView;
use crate::{App, Msg};
use iced::widget::{
    button, checkbox, column, container, image, progress_bar, row, scrollable, stack, text, Space,
};
use iced::{
    gradient, Alignment, Background, Border, Color, ContentFit, Element, Font, Length, Padding, Shadow, Theme,
};

const BG: Color = Color::from_rgb(0.0, 0.0, 0.0);
const PANEL: Color = Color::from_rgb(0.051, 0.051, 0.051);
const PANEL2: Color = Color::from_rgb(0.11, 0.11, 0.11);
const LINE: Color = Color::from_rgb(0.18, 0.18, 0.18);
const TEXT: Color = Color::from_rgb(0.95, 0.95, 0.95);
const MUTED: Color = Color::from_rgb(0.55, 0.55, 0.55);
const WHITE: Color = Color::WHITE;
const BLACK: Color = Color::BLACK;
const ERR: Color = Color::from_rgb(1.0, 0.42, 0.42);

static COVERS: include_dir::Dir = include_dir::include_dir!("$CARGO_MANIFEST_DIR/assets/covers");
const LOGO: &[u8] = include_bytes!("../assets/logo.png");

/// Banner shipped inside the launcher (`covers/x.jpg` in the catalog).
pub fn embedded_cover(path: &str) -> Option<image::Handle> {
    let name = path.strip_prefix("covers/")?;
    COVERS.get_file(name).map(|f| image::Handle::from_bytes(f.contents().to_vec()))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Store,
    Busy,
    Unsupported,
    Available,
    Unavailable,
    Update,
    Installed,
}

pub fn status(app: &App, t: &TitleView) -> Status {
    let id = &t.title.id;
    let rel = app.releases.get(id).and_then(|r| r.as_ref().ok());
    if t.title.store_url.is_some() {
        return Status::Store;
    }
    if app.progress.contains_key(id) {
        return Status::Busy;
    }
    if !t.supported {
        return Status::Unsupported;
    }
    let Some(installed) = &t.installed else {
        return if rel.is_some() { Status::Available } else { Status::Unavailable };
    };
    match rel {
        Some(r) if r.tag != installed.version => Status::Update,
        _ => Status::Installed,
    }
}

fn mb(n: u64) -> String {
    format!("{:.1} MB", n as f64 / 1_048_576.0)
}

fn bold() -> Font {
    Font { weight: iced::font::Weight::Bold, ..Font::DEFAULT }
}

fn semibold() -> Font {
    Font { weight: iced::font::Weight::Semibold, ..Font::DEFAULT }
}

// ---- button styles -------------------------------------------------------------------------

fn primary_style(_: &Theme, status: button::Status) -> button::Style {
    let bg = match status {
        button::Status::Hovered | button::Status::Pressed => Color::from_rgb(0.85, 0.85, 0.85),
        button::Status::Disabled => Color::from_rgb(0.4, 0.4, 0.4),
        _ => WHITE,
    };
    button::Style { background: Some(Background::Color(bg)), text_color: BLACK, border: Border { radius: 6.0.into(), ..Default::default() }, ..Default::default() }
}

fn secondary_style(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(PANEL2)),
        text_color: TEXT,
        border: Border { color: if hovered { MUTED } else { LINE }, width: 1.0, radius: 6.0.into() },
        ..Default::default()
    }
}

fn ghost_style(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style { background: None, text_color: if hovered { TEXT } else { MUTED }, ..Default::default() }
}

fn danger_style(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Color(if hovered { Color::from_rgb(1.0, 0.55, 0.55) } else { ERR })),
        text_color: BLACK,
        border: Border { radius: 6.0.into(), ..Default::default() },
        ..Default::default()
    }
}

fn item_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| {
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        button::Style {
            background: (active || hovered).then_some(Background::Color(PANEL2)),
            text_color: TEXT,
            border: Border { radius: 6.0.into(), ..Default::default() },
            ..Default::default()
        }
    }
}

fn kofi_style(_: &Theme, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: Some(Background::Gradient(gradient::Linear::new(std::f32::consts::FRAC_PI_4).add_stop(0.0, WHITE).add_stop(1.0, Color::from_rgb(0.81, 0.81, 0.81)).into())),
        text_color: BLACK,
        border: Border { color: WHITE, width: 1.0, radius: 10.0.into() },
        shadow: if hovered { Shadow { color: Color { a: 0.35, ..WHITE }, offset: iced::Vector::new(0.0, 4.0), blur_radius: 18.0 } } else { Shadow::default() },
        ..Default::default()
    }
}

fn box_style(bg: Color, border: Option<Color>, radius: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(bg)),
        border: Border { color: border.unwrap_or(Color::TRANSPARENT), width: if border.is_some() { 1.0 } else { 0.0 }, radius: radius.into() },
        ..Default::default()
    }
}

fn btn<'a>(label: &'a str, msg: Msg, style: fn(&Theme, button::Status) -> button::Style) -> button::Button<'a, Msg> {
    button(text(label).font(semibold())).on_press(msg).padding([10, 22]).style(style)
}

// ---- views ---------------------------------------------------------------------------------

pub fn view(app: &App) -> Element<'_, Msg> {
    let main = row![sidebar(app), detail(app)];
    let mut layers = stack![container(main).width(Length::Fill).height(Length::Fill).style(|_| box_style(BG, None, 0.0))];
    if let Some(u) = &app.launcher_update {
        layers = layers.push(
            container(update_bar(app, u))
                .width(Length::Fill)
                .align_x(Alignment::End)
                .padding(Padding { top: 12.0, right: 16.0, bottom: 0.0, left: 0.0 }),
        );
    }
    layers.into()
}

fn update_bar<'a>(app: &'a App, u: &'a crate::updater::Update) -> Element<'a, Msg> {
    let body: Element<Msg> = if let (false, Some(e)) = (app.launcher_updating, &app.launcher_update_error.as_ref().filter(|e| !e.is_empty())) {
        column![
            text(format!("Update failed: {e}")).color(ERR).size(13),
            button(text("Retry").size(13)).on_press(Msg::DoLauncherUpdate).padding([6, 14]).style(primary_style),
        ]
        .spacing(6)
        .into()
    } else {
        let go = if app.launcher_updating {
            button(text("Updating…").size(13)).padding([6, 14]).style(primary_style)
        } else {
            button(text("Update & restart").size(13).font(semibold())).on_press(Msg::DoLauncherUpdate).padding([6, 14]).style(|t, s| {
                let mut st = primary_style(t, s);
                st.background = Some(Background::Color(BLACK));
                st.text_color = WHITE;
                st
            })
        };
        row![text(format!("Rosace Launcher {} is available.", u.version)).color(BLACK), go].spacing(12).align_y(Alignment::Center).into()
    };
    container(body).padding(Padding { top: 8.0, right: 10.0, bottom: 8.0, left: 16.0 }).style(|_| container::Style {
        background: Some(Background::Color(WHITE)),
        border: Border { radius: 8.0.into(), ..Default::default() },
        shadow: Shadow { color: Color { a: 0.6, ..BLACK }, offset: iced::Vector::new(0.0, 6.0), blur_radius: 24.0 },
        ..Default::default()
    })
    .into()
}

fn sidebar(app: &App) -> Element<'_, Msg> {
    let brand = row![
        image(image::Handle::from_bytes(LOGO)).width(36).height(36),
        text("Rosace Launcher").size(18).font(semibold())
    ]
    .spacing(10)
    .align_y(Alignment::Center)
    .padding(Padding { top: 4.0, right: 10.0, bottom: 18.0, left: 10.0 });

    let mut list = column![].spacing(2);
    for (cat, label) in [("game", "GAMES"), ("software", "SOFTWARE")] {
        let group: Vec<&TitleView> = app.titles.iter().filter(|t| t.title.category == cat).collect();
        if group.is_empty() {
            continue;
        }
        list = list.push(container(text(label).size(11).color(MUTED)).padding(Padding { top: 12.0, right: 12.0, bottom: 4.0, left: 12.0 }));
        for t in group {
            list = list.push(item(app, t));
        }
    }
    if app.titles.is_empty() {
        list = list.push(container(text(if app.loading { "Loading…" } else { "No titles in catalog." }).color(MUTED)).padding(12));
    }

    let kofi = button(
        row![
            text("\u{2615}").size(22),
            column![text("Fund my projects").size(14).font(bold()), text("Support me on Ko-fi").size(11).color(Color::from_rgb(0.27, 0.27, 0.27))],
            Space::new().width(Length::Fill),
            text("\u{2665}").size(16),
        ]
        .spacing(10)
        .align_y(Alignment::Center),
    )
    .on_press(Msg::Kofi)
    .width(Length::Fill)
    .padding([10, 12])
    .style(kofi_style);

    let content = column![
        brand,
        scrollable(list).height(Length::Fill),
        container(kofi).padding([10, 4]),
        container(checkbox(app.settings.auto_update).label("Auto-update").on_toggle(Msg::AutoUpdate).size(16).text_size(13)).padding([4, 12]),
        button(text("Refresh").size(13)).on_press(Msg::Refresh).padding([8, 12]).style(ghost_style),
        container(text(format!("Rosace Launcher v{}", env!("CARGO_PKG_VERSION"))).size(11).color(MUTED)).width(Length::Fill).align_x(Alignment::Center).padding([4, 0]),
    ]
    .spacing(2);

    container(content)
        .width(260)
        .height(Length::Fill)
        .padding([16, 10])
        .style(|_| container::Style {
            background: Some(Background::Color(PANEL)),
            border: Border { color: LINE, width: 1.0, radius: 0.0.into() },
            ..Default::default()
        })
        .into()
}

fn item<'a>(app: &'a App, t: &'a TitleView) -> Element<'a, Msg> {
    let active = app.selected.as_deref() == Some(t.title.id.as_str());
    let badge: Option<Element<Msg>> = match status(app, t) {
        Status::Update => Some(container(text("Update").size(11).color(BLACK)).padding([1, 7]).style(|_| box_style(WHITE, None, 99.0)).into()),
        Status::Installed => Some(container(text("Installed").size(11).color(Color::from_rgb(0.85, 0.85, 0.85))).padding([1, 7]).style(|_| box_style(BG, Some(LINE), 99.0)).into()),
        Status::Busy => Some(container(text("…").size(11).color(MUTED)).padding([1, 7]).style(|_| box_style(BG, Some(LINE), 99.0)).into()),
        Status::Store => Some(container(text("Steam").size(11).color(MUTED)).padding([1, 7]).style(|_| box_style(BG, Some(LINE), 99.0)).into()),
        _ => None,
    };
    let mut r = row![text(&t.title.name).size(14), Space::new().width(Length::Fill)].align_y(Alignment::Center);
    if let Some(b) = badge {
        r = r.push(b);
    }
    // The white bar on the left of the selected entry.
    let bar = container(Space::new().width(3).height(Length::Fill)).style(move |_| box_style(if active { WHITE } else { Color::TRANSPARENT }, None, 0.0));
    row![bar, button(r).on_press(Msg::Select(t.title.id.clone())).width(Length::Fill).padding([10, 12]).style(item_style(active))].height(Length::Shrink).into()
}

fn detail(app: &App) -> Element<'_, Msg> {
    if let Some(e) = &app.load_error {
        return container(text(e).color(ERR)).padding(24).width(Length::Fill).into();
    }
    let Some(t) = app.selected.as_ref().and_then(|id| app.titles.iter().find(|t| &t.title.id == id)) else {
        return container(text(if app.loading { "Loading…" } else { "Select a title." }).color(MUTED)).padding(24).width(Length::Fill).into();
    };
    let id = &t.title.id;
    let s = status(app, t);
    let rel = app.releases.get(id).and_then(|r| r.as_ref().ok());
    let rel_err = app.releases.get(id).and_then(|r| r.as_ref().err());

    let mut actions = row![].spacing(10).align_y(Alignment::Center);
    match s {
        Status::Store => {
            let url = t.title.store_url.clone().unwrap_or_default();
            actions = actions.push(button(text("View on Steam").size(16).font(semibold())).on_press(Msg::Store(url)).padding([12, 40]).style(primary_style));
            actions = actions.push(text("Available on Steam").color(MUTED));
        }
        Status::Busy => {
            let p = &app.progress[id];
            let (pct, label) = if p.stage == "extract" {
                (100.0, "Installing…".to_string())
            } else {
                let pct = if p.total > 0 { p.downloaded as f32 / p.total as f32 * 100.0 } else { 0.0 };
                (pct, format!("Downloading {} / {} ({}%)", mb(p.downloaded), mb(p.total), pct.round() as u32))
            };
            actions = actions.push(
                column![
                    progress_bar(0.0..=100.0, pct).girth(6).style(|_| progress_bar::Style {
                        background: Background::Color(PANEL2),
                        bar: Background::Color(WHITE),
                        border: Border { radius: 99.0.into(), ..Default::default() },
                    }),
                    text(label).color(MUTED).size(13)
                ]
                .spacing(6)
                .width(Length::Fill),
            );
        }
        Status::Installed | Status::Update => {
            let verb = if t.title.category == "software" { "Open" } else { "Play" };
            actions = actions.push(button(text(verb).size(16).font(semibold())).on_press(Msg::Launch(id.clone())).padding([12, 40]).style(primary_style));
            if let (Status::Update, Some(r)) = (s, rel) {
                actions = actions.push(button(text(format!("Update to {}", r.tag)).font(semibold())).on_press(Msg::Install(id.clone())).padding([10, 22]).style(secondary_style));
            }
            actions = actions.push(btn("Open folder", Msg::Folder(id.clone()), secondary_style));
            if app.confirm_uninstall.as_deref() == Some(id.as_str()) {
                actions = actions.push(text(format!("Uninstall {}?", t.title.name)).color(MUTED));
                actions = actions.push(btn("Yes, uninstall", Msg::Uninstall(id.clone()), danger_style));
                actions = actions.push(btn("Cancel", Msg::CancelUninstall, ghost_style));
            } else {
                actions = actions.push(btn("Uninstall", Msg::AskUninstall(id.clone()), ghost_style));
            }
        }
        Status::Available => {
            actions = actions.push(button(text("Install").size(16).font(semibold())).on_press(Msg::Install(id.clone())).padding([12, 40]).style(primary_style));
            if let Some(r) = rel {
                actions = actions.push(text(mb(r.size)).color(MUTED));
            }
        }
        Status::Unsupported => actions = actions.push(text("Not available on this system yet.").color(MUTED)),
        Status::Unavailable => actions = actions.push(text(rel_err.cloned().unwrap_or_else(|| "Checking for releases…".into())).color(MUTED)),
    }

    // Banner with the title over a bottom fade; plain panel when the title has no cover.
    let hero_h = 260;
    let title_text = container(text(&t.title.name).size(40).font(semibold()).color(TEXT)).width(Length::Fill).height(hero_h).align_y(Alignment::End).padding(Padding { top: 0.0, right: 36.0, bottom: 28.0, left: 36.0 });
    let hero: Element<Msg> = match app.covers.get(id) {
        Some(h) => stack![
            image(h.clone()).width(Length::Fill).height(hero_h).content_fit(ContentFit::Cover),
            container(Space::new().width(Length::Fill).height(hero_h)).style(|_| container::Style {
                background: Some(Background::Gradient(
                    gradient::Linear::new(std::f32::consts::PI)
                        .add_stop(0.0, Color { a: 0.0, ..BLACK })
                        .add_stop(0.35, Color { a: 0.0, ..BLACK })
                        .add_stop(0.65, Color { a: 0.27, ..BLACK })
                        .add_stop(1.0, Color { a: 0.85, ..BLACK })
                        .into()
                )),
                ..Default::default()
            }),
            title_text,
        ]
        .width(Length::Fill)
        .height(hero_h)
        .into(),
        None => stack![
            container(Space::new().width(Length::Fill).height(hero_h)).style(|_| container::Style {
                background: Some(Background::Gradient(gradient::Linear::new(std::f32::consts::FRAC_PI_4).add_stop(0.0, Color::from_rgb(0.16, 0.16, 0.16)).add_stop(0.7, BLACK).into())),
                ..Default::default()
            }),
            title_text,
        ]
        .width(Length::Fill)
        .height(hero_h)
        .into(),
    };

    let mut body = column![container(actions).width(Length::Fill)].spacing(18).max_width(900);
    if let Some(e) = app.errors.get(id) {
        body = body.push(text(e).color(ERR));
    }
    body = body.push(text(&t.title.description));
    let mut meta = String::new();
    if let Some(i) = &t.installed {
        meta += &format!("Installed: {} · ", i.version);
    }
    if let Some(r) = rel {
        meta += &format!("Latest: {}", r.name);
        if let Some(d) = &r.published_at {
            meta += &format!(" · {}", d.get(..10).unwrap_or(d));
        }
    }
    if !meta.is_empty() {
        body = body.push(text(meta).color(MUTED));
    }
    if let Some(r) = rel.filter(|r| !r.notes.trim().is_empty()) {
        body = body.push(text("Patch notes").size(18).font(semibold()));
        body = body.push(container(text(&r.notes)).padding([14, 18]).width(Length::Fill).style(|_| box_style(PANEL, Some(LINE), 6.0)));
    }

    scrollable(column![hero, container(body).padding([24, 36])]).width(Length::Fill).height(Length::Fill).into()
}
