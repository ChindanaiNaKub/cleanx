//! cleanX: a disk-space dashboard and a safe-clean report, drawn with Rune.
//! `cargo run --` opens the window. `-- --snap out.png` renders headlessly.

#![forbid(unsafe_code)]

mod gfx;
mod scan;
mod session;

use rune::tok::{self, motion, radius, space};
use rune::{
    AppSpec, Color, Cx, El, LineCap, ScrollSpec, Shape, canvas, col, effect, el, oklch, row, text,
};

/// One type scale for the app; Rune's tokens are 11.5–13 px, too small here.
mod type_ {
    use rune::tok::TextStyle;
    pub const BODY: TextStyle = TextStyle::new(14.0, 400.0);
    pub const LABEL: TextStyle = TextStyle::new(14.0, 500.0);
    pub const SECTION: TextStyle = TextStyle::new(13.0, 600.0);
    pub const TITLE: TextStyle = TextStyle::new(16.0, 650.0);
    pub const HEADING: TextStyle = TextStyle::new(28.0, 650.0);
    pub const VALUE: TextStyle = TextStyle::new(22.0, 650.0);
    pub const HERO: TextStyle = TextStyle::new(50.0, 650.0);
    pub const MONO_CAPTION: TextStyle = TextStyle::new(13.0, 500.0).mono();
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Space,
    Clean,
}

struct State {
    session: session::Session,
    tab: Tab,
    notice: Option<String>,
    /// The `isolated` scene sets this to render effects inside an isolated layer.
    isolate: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            session: session::Session::new(),
            tab: Tab::Space,
            notice: None,
            isolate: false,
        }
    }
}

/// The real entry point. Purging the last session's quarantine is destructive,
/// so it happens here and not in `Default`, which tests and scenes also call.
fn init() -> State {
    let mut s = State::default();
    let tooling = std::env::args().any(|a| {
        a.starts_with("--snap")
            || a.starts_with("--tree")
            || a.starts_with("--video")
            || a.starts_with("--out")
            || a.starts_with("--script")
            || a == "--list-scenes"
    });
    if !tooling {
        s.session.purge_previous_session();
    }
    s
}

fn mood(free: f32) -> (&'static str, Color) {
    if free > 0.4 {
        ("Plenty of room", oklch(0.82, 0.15, 155.0))
    } else if free > 0.2 {
        ("Getting cosy", oklch(0.84, 0.14, 105.0))
    } else if free > 0.1 {
        ("Running low", oklch(0.84, 0.15, 65.0))
    } else {
        ("Almost full", oklch(0.78, 0.18, 30.0))
    }
}

/// An effect slot: `mark` picks the shader, `data` carries its live numbers.
/// The slot fills its parent so effects follow a resized window.
fn fx(mark: u32, data: [f32; 4]) -> El {
    effect(mark)
        .effect_data(data)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
}

/// Wedges around a common centre. Each wedge grows in turn during a scan.
/// Rune has no chart primitive, so the sunburst is generated polygon geometry.
fn sunburst(progress: f32, hue: f32) -> Shape {
    const SLICES: u32 = 9;
    const STEP: f32 = std::f32::consts::TAU / SLICES as f32;
    let mut shape = Shape::new().view_box(120.0, 120.0);
    for i in 0..SLICES {
        let grow = (progress * SLICES as f32 - i as f32).clamp(0.0, 1.0);
        if grow <= 0.0 {
            continue;
        }
        let a0 = i as f32 * STEP - std::f32::consts::FRAC_PI_2;
        let a1 = a0 + STEP * 0.82;
        let inner = 19.0;
        let outer = inner + 39.0 * grow;
        let mut points = Vec::new();
        for k in 0..=8 {
            let a = a0 + (a1 - a0) * k as f32 / 8.0;
            points.push([60.0 + outer * a.cos(), 60.0 + outer * a.sin()]);
        }
        for k in (0..=8).rev() {
            let a = a0 + (a1 - a0) * k as f32 / 8.0;
            points.push([60.0 + inner * a.cos(), 60.0 + inner * a.sin()]);
        }
        let lightness = 0.48 + 0.26 * i as f32 / SLICES as f32;
        shape = shape.fill(
            points,
            oklch(lightness, 0.16, hue + i as f32 * 6.0).alpha(0.78),
        );
    }
    shape
}

/// A vector tick. Centred on the view box so it stays balanced at any size.
fn checkmark(size: f32, color: Color) -> Shape {
    Shape::new()
        .view_box(size, size)
        .line(
            [size * 0.27, size * 0.53],
            [size * 0.43, size * 0.69],
            size * 0.12,
            LineCap::Round,
            color,
        )
        .line(
            [size * 0.43, size * 0.69],
            [size * 0.75, size * 0.31],
            size * 0.12,
            LineCap::Round,
            color,
        )
}

fn view(s: &State, _: &mut Cx<State>) -> El {
    let t = tok::theme(true);
    let hair = 1.0;
    col()
        .key("app")
        .role("window")
        .w_full()
        .h_full()
        .fill(t.bg)
        .child(
            fx(gfx::MESH, [0.0, 0.0, 0.0, 0.0])
                .key("ambient-lava")
                .opacity(0.12),
        )
        .child(
            row()
                .key("shell")
                .grow(1.0)
                .min_h(0.0)
                .child(sidebar(
                    s,
                    t.card,
                    t.text,
                    t.text_secondary,
                    t.indicator,
                    hair,
                ))
                .child(
                    col()
                        .key("main")
                        .grow(1.0)
                        .min_w(0.0)
                        .min_h(0.0)
                        .px(space::S6)
                        .pt(space::S6)
                        .pb(space::S6)
                        .gap(space::S6)
                        .child(match s.tab {
                            Tab::Space => col()
                                .key("space-stage")
                                .center_y()
                                .grow(1.0)
                                .min_w(0.0)
                                .min_h(0.0)
                                .child(fx(gfx::MESH, [0.0, 0.0, 0.0, 0.0]).opacity(0.14))
                                .child(space_view(
                                    s,
                                    t.card,
                                    t.text,
                                    t.text_secondary,
                                    t.separator,
                                    hair,
                                )),
                            Tab::Clean => {
                                clean_view(s, t.card, t.text, t.text_secondary, t.separator, hair)
                            }
                        }),
                ),
        )
        .child(match s.session.burst_time() {
            Some(t0) => effect(gfx::SPARK)
                .effect_data([t0 as f32, 0.0, 0.0, 0.0])
                .key("confetti")
                .size(260.0, 180.0)
                .center()
                .radius(radius::LG)
                .effect_bleed(60.0),
            None => el().key("confetti-off").h(0.0),
        })
}

fn sidebar(
    s: &State,
    card: Color,
    ink: Color,
    secondary: Color,
    indicator: Color,
    hair: f32,
) -> El {
    let t = tok::theme(true);
    let mut tabs = col().key("tabs").gap(4.0).mt(space::S6);
    for (tab, key, label) in [
        (Tab::Space, "tab-space", "Space"),
        (Tab::Clean, "tab-clean", "Clean"),
    ] {
        let on = s.tab == tab;
        tabs = tabs.child(
            el().key(key)
                .role("tab")
                .label(label)
                .checked(on)
                .focusable(true)
                .h(38.0)
                .px(space::S3)
                .radius(radius::MD)
                .fill(if on { card } else { Color::TRANSPARENT })
                .hover(secondary.alpha(0.08))
                .on_press(move |s: &mut State| s.tab = tab)
                .center_y()
                .child(
                    text(label)
                        .font(type_::TITLE)
                        .color(if on { ink } else { secondary })
                        .a11y_hidden(),
                ),
        );
    }
    col()
        .key("sidebar")
        .w(216.0)
        .h_full()
        .px(space::S6)
        .pt(space::S6)
        .fill(t.bg.mix(Color::BLACK, 0.25))
        .border(hair, t.separator)
        .child(
            col()
                .gap(2.0)
                .child(text("cleanX").key("brand").font(type_::TITLE).color(ink))
                .child(
                    text("space, made visible")
                        .font(type_::LABEL)
                        .color(secondary),
                ),
        )
        .child(tabs)
        .child(
            col()
                .key("scan-summary")
                .grow(1.0)
                .gap(2.0)
                .mt(space::S6)
                .child(
                    text("LAST SCAN")
                        .font(type_::SECTION)
                        .color(secondary.alpha(0.7)),
                )
                .child(
                    text(format!("{} items", s.session.candidates().len()))
                        .font(type_::VALUE)
                        .color(ink),
                )
                .child(
                    text(
                        s.session
                            .current_volume()
                            .map(|v| v.mount.as_str())
                            .unwrap_or("no disk"),
                    )
                    .font(type_::LABEL)
                    .color(secondary)
                    .truncate(),
                ),
        )
        .child(
            el().key("accent-line")
                .h(2.0)
                .w(28.0)
                .radius(1.0)
                .fill(indicator)
                .mb(space::S3),
        )
}

fn space_view(
    s: &State,
    card: Color,
    ink: Color,
    secondary: Color,
    separator: Color,
    hair: f32,
) -> El {
    let Some(v) = s.session.current_volume() else {
        return col().key("space").grow(1.0).center().child(
            text("No disks found. cleanX reads disks with `df`.")
                .font(type_::BODY)
                .color(secondary),
        );
    };
    let free = scan::fraction(v.free, v.total);
    let used = scan::fraction(v.used, v.total);
    let (mood_text, mood_color) = mood(free);
    let hue = 150.0 - (1.0 - free) * 125.0;
    let orb_accent = oklch(0.72, 0.17, hue);

    let mut by_kind: Vec<(scan::Kind, u64)> = Vec::new();
    for c in s.session.candidates() {
        match by_kind.iter_mut().find(|(k, _)| *k == c.target.kind) {
            Some(entry) => entry.1 += c.target.bytes,
            None => by_kind.push((c.target.kind, c.target.bytes)),
        }
    }
    by_kind.sort_by(|a, b| b.1.cmp(&a.1));

    let mut pills = row().key("volumes").gap(6.0).w_full();
    if s.session.volumes().len() > 4 {
        pills = pills.scroll_x();
    }
    for (i, vol) in s.session.volumes().iter().enumerate() {
        let on = i == s.session.volume_index();
        let short: String = vol.name.chars().take(11).collect();
        let shown = if vol.name.chars().count() > 11 {
            format!("{short}…")
        } else {
            short
        };
        pills = pills.child(
            el().key(format!("vol-{i}"))
                .role("radio")
                .label(vol.name.clone())
                .checked(on)
                .focusable(true)
                .h(26.0)
                .px(space::S3)
                .radius(13.0)
                .fill(if on { card } else { Color::TRANSPARENT })
                .border(hair, if on { orb_accent } else { separator })
                .on_press(move |s: &mut State| s.session.select_volume(i))
                .center_y()
                .child(
                    text(shown)
                        .font(type_::LABEL)
                        .color(if on { ink } else { secondary })
                        .a11y_hidden(),
                ),
        );
    }

    col()
        .key("space")
        .grow(1.0)
        .w_full()
        .max_w(1480.0)
        .min_h(0.0)
        .gap(space::S2)
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .child(
            row()
                .key("space-head")
                .gap(space::S3)
                .center_y()
                .child(
                    col()
                        .gap(2.0)
                        .child(
                            text("Free space")
                                .key("title")
                                .font(type_::HEADING)
                                .color(ink),
                        )
                        .child(text(&v.mount).font(type_::LABEL).color(secondary)),
                )
                .child(el().grow(1.0))
                .child(
                    el().key("scan-now")
                        .h(32.0)
                        .px(space::S3)
                        .radius(radius::MD)
                        .fill(card)
                        .border(hair, separator)
                        .center()
                        .on_press(|s: &mut State| {
                            s.session.scan_now();
                            s.notice = None;
                        })
                        .child(text("Scan now").font(type_::LABEL).color(ink)),
                ),
        )
        .child(pills)
        .child(
            row()
                .key("hero")
                .gap(space::S6)
                .center_y()
                .child(
                    el().key("orb")
                        .size(212.0, 212.0)
                        .radius(106.0)
                        .fill(Color::TRANSPARENT)
                        .radial_gradient(
                            [0.38, 0.3],
                            150.0,
                            [
                                (0.0, oklch(0.92, 0.06, hue).alpha(0.34)),
                                (0.5, orb_accent.alpha(0.34)),
                                (1.0, oklch(0.4, 0.16, hue).alpha(0.34)),
                            ],
                        )
                        .shadow(0.0, 26.0, orb_accent.alpha(0.22))
                        .motion(motion::DEFAULT)
                        .focusable(true)
                        .on_press(|s: &mut State| s.tab = Tab::Clean)
                        .center()
                        .child(
                            fx(gfx::LAVA, [0.26, 0.45, 0.0, 0.0])
                                .key("orb-lava")
                                .radius(106.0),
                        )
                        .child(
                            fx(gfx::GLOW, [64.0, 0.9, 1.0 - free, 0.0])
                                .key("orb-glow")
                                .radius(106.0)
                                .effect_bleed(160.0),
                        )
                        .child(
                            col()
                                .center()
                                .gap(0.0)
                                .child(
                                    text(format!("{:.0}%", free * 100.0))
                                        .key("free-pct")
                                        .font(type_::HERO)
                                        .color(Color::WHITE),
                                )
                                .child(
                                    text("FREE")
                                        .font(type_::SECTION)
                                        .color(Color::WHITE.alpha(0.85)),
                                ),
                        ),
                )
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(space::S3)
                        .child(
                            canvas(sunburst(s.session.scan_progress(), hue))
                                .key("sunburst")
                                .size(120.0, 120.0),
                        )
                        .child(text("scan coverage").font(type_::SECTION).color(secondary))
                        .child(
                            text(mood_text)
                                .key("mood")
                                .font(type_::VALUE)
                                .color(mood_color),
                        )
                        .child(
                            text("How much of this disk you can still grow into.")
                                .font(type_::BODY)
                                .color(secondary)
                                .truncate(),
                        ),
                ),
        )
        .child(
            col()
                .key("bar-wrap")
                .gap(space::S2)
                .child(
                    row()
                        .key("bar")
                        .h(16.0)
                        .w_full()
                        .radius(8.0)
                        .fill(tok::theme(true).control_off)
                        .child(
                            el().key("used-bar")
                                .h(16.0)
                                .w_frac(used)
                                .radius(8.0)
                                .fill(orb_accent.alpha(0.85))
                                .motion(motion::DEFAULT),
                        )
                        .child(
                            el().key("free-bar")
                                .h(16.0)
                                .w_frac(free)
                                .radius(8.0)
                                .fill(oklch(0.8, 0.13, 155.0).alpha(0.9))
                                .motion(motion::DEFAULT),
                        ),
                )
                .child(
                    row()
                        .gap(space::S2)
                        .child(text("used").font(type_::LABEL).color(secondary))
                        .child(
                            text(scan::human(v.used))
                                .font(type_::MONO_CAPTION)
                                .color(ink),
                        )
                        .child(el().w(space::S3))
                        .child(text("free").font(type_::LABEL).color(secondary))
                        .child(
                            text(scan::human(v.free))
                                .font(type_::MONO_CAPTION)
                                .color(ink),
                        ),
                ),
        )
        .child(
            row()
                .key("stats")
                .gap(space::S3)
                .child(stat(
                    "FREE",
                    &scan::human(v.free),
                    mood_color,
                    card,
                    secondary,
                    hair,
                ))
                .child(stat(
                    "USED",
                    &scan::human(v.used),
                    orb_accent,
                    card,
                    secondary,
                    hair,
                ))
                .child(stat(
                    "TOTAL",
                    &scan::human(v.total),
                    ink,
                    card,
                    secondary,
                    hair,
                )),
        )
        .child(
            row()
                .key("reclaim")
                .opacity(if s.isolate { 0.99 } else { 1.0 })
                .h(64.0)
                .px(space::S3)
                .gap(space::S3)
                .center_y()
                .radius(radius::LG)
                .fill(card)
                .border(hair, separator)
                .child(
                    fx(gfx::MESH, [0.0, 0.0, 0.0, 0.0])
                        .key("reclaim-mesh")
                        .radius(radius::LG),
                )
                .child(
                    col()
                        .gap(1.0)
                        .child(
                            text("SELECTED TO RECLAIM")
                                .font(type_::SECTION)
                                .color(secondary),
                        )
                        .child(
                            text("open Clean to review")
                                .font(type_::LABEL)
                                .color(secondary),
                        )
                        .child(
                            text(format!(
                                "held in quarantine {} · freed {}",
                                scan::human(s.session.held_bytes()),
                                scan::human(s.session.freed_bytes())
                            ))
                            .key("held-total")
                            .font(type_::LABEL)
                            .color(oklch(0.82, 0.14, 155.0)),
                        ),
                )
                .child(
                    el().grow(1.0).child(
                        text(scan::human(s.session.reclaimable()))
                            .font(type_::VALUE)
                            .color(oklch(0.82, 0.14, 155.0))
                            .align(rune::Align::End),
                    ),
                )
                .child(
                    el().key("go-clean")
                        .h(32.0)
                        .px(space::S3)
                        .radius(radius::MD)
                        .fill(tok::accent(4, true).alpha(0.22))
                        .center()
                        .on_press(|s: &mut State| s.tab = Tab::Clean)
                        .child(
                            text("Review")
                                .font(type_::LABEL)
                                .color(oklch(0.85, 0.14, 155.0)),
                        ),
                ),
        )
        .child(
            col()
                .key("by-kind")
                .h(72.0)
                .px(space::S3)
                .py(space::S2)
                .gap(space::S2)
                .radius(radius::LG)
                .fill(card)
                .border(hair, separator)
                .child(
                    text("RECLAIMABLE BY TYPE")
                        .font(type_::SECTION)
                        .color(secondary),
                )
                .child({
                    let mut chips = row().key("kind-chips").gap(space::S2).grow(1.0);
                    for (kind, bytes) in &by_kind {
                        chips = chips.child(
                            col()
                                .grow(1.0)
                                .min_w(0.0)
                                .gap(2.0)
                                .child(text(kind.label()).font(type_::SECTION).color(secondary))
                                .child(
                                    text(scan::human(*bytes))
                                        .font(type_::TITLE)
                                        .color(orb_accent),
                                ),
                        );
                    }
                    chips
                }),
        )
        .child(
            col()
                .key("disks")
                .min_h(200.0)
                .max_h(360.0)
                .grow(1.0)
                .px(space::S3)
                .py(space::S2)
                .gap(space::S2)
                .radius(radius::LG)
                .fill(card)
                .border(hair, separator)
                .child(text("DISKS").font(type_::SECTION).color(secondary))
                .child({
                    let mut rows = col().key("disk-rows").grow(1.0).gap(space::S2);
                    for (i, vol) in s.session.volumes().iter().enumerate() {
                        let on = i == s.session.volume_index();
                        let frac = scan::fraction(vol.free, vol.total);
                        rows =
                            rows.child(
                                row()
                                    .key(format!("disk-{i}"))
                                    .grow(1.0)
                                    .gap(space::S3)
                                    .center_y()
                                    .child(
                                        text(&vol.name)
                                            .font(type_::LABEL)
                                            .color(if on { ink } else { secondary })
                                            .truncate(),
                                    )
                                    .child(el().grow(1.0))
                                    .child(
                                        el().w(140.0)
                                            .h(4.0)
                                            .radius(2.0)
                                            .fill(tok::theme(true).control_off)
                                            .child(el().h(4.0).w_frac(frac).radius(2.0).fill(
                                                if on { orb_accent } else { secondary.alpha(0.6) },
                                            )),
                                    )
                                    .child(
                                        text(format!("{:.0}% free", frac * 100.0))
                                            .font(type_::LABEL)
                                            .color(if on { orb_accent } else { secondary }),
                                    ),
                            );
                    }
                    rows
                }),
        )
}

fn stat(label: &str, value: &str, accent: Color, card: Color, secondary: Color, hair: f32) -> El {
    col()
        .key(label)
        .grow(1.0)
        .min_w(0.0)
        .gap(4.0)
        .padding(space::S2)
        .radius(radius::LG)
        .fill(card)
        .border(hair, secondary.alpha(0.1))
        .child(text(label).font(type_::SECTION).color(secondary))
        .child(text(value).font(type_::VALUE).color(accent))
        .child(el().h(2.0).w(24.0).radius(1.0).fill(accent.alpha(0.7)))
}

fn clean_view(
    s: &State,
    card: Color,
    ink: Color,
    secondary: Color,
    separator: Color,
    hair: f32,
) -> El {
    let armed = s.session.armed();
    let chosen = s.session.reclaimable();
    let n = s.session.chosen_count();
    let label = if armed {
        format!("Confirm remove {n}")
    } else {
        "Clean selected".into()
    };
    let btn_color = if armed {
        oklch(0.7, 0.17, 30.0)
    } else {
        tok::accent(0, true)
    };
    let summary_fx = if armed {
        fx(gfx::FIRE, [18.0, 1.25, 10.0, 0.0])
            .key("clean-fire")
            .radius(radius::LG)
            .effect_bleed(28.0)
    } else {
        el().key("clean-fire-off").h(0.0)
    };

    let max_bytes = s
        .session
        .candidates()
        .iter()
        .map(|c| c.target.bytes)
        .max()
        .unwrap_or(1)
        .max(1);
    let mut list = col().key("clean-list").gap(6.0);
    if s.session.candidates().is_empty() {
        list = list.child(
            col()
                .center()
                .h(120.0)
                .gap(6.0)
                .child(text("Nothing to clean here").font(type_::TITLE).color(ink))
                .child(
                    text("No known caches, temp files or OS junk were found under your home.")
                        .font(type_::BODY)
                        .color(secondary),
                ),
        );
    }
    for (i, c) in s.session.candidates().iter().enumerate() {
        let target = &c.target;
        let on = c.chosen;
        let badge = if on {
            oklch(0.78, 0.14, 155.0)
        } else {
            secondary
        };
        let tag_color = match target.impact {
            scan::Impact::Safe => oklch(0.78, 0.14, 155.0),
            scan::Impact::Caution | scan::Impact::ViewOnly => oklch(0.82, 0.13, 70.0),
            scan::Impact::HandsOff => oklch(0.72, 0.16, 30.0),
        };
        let bar_w = (90.0 * scan::fraction(target.bytes, max_bytes)).max(2.0);
        list = list.child(
            row()
                .key(format!("row-{i}"))
                .h(66.0)
                .px(space::S3)
                .gap(space::S3)
                .center_y()
                .radius(radius::MD)
                .fill(card)
                .hover(secondary.alpha(0.06))
                .press(secondary.alpha(0.1))
                .press_scale(0.985)
                .focusable(true)
                .on_press(move |s: &mut State| s.session.toggle(i))
                .child(
                    fx(
                        gfx::ROW | ((i as u32) << 8),
                        [if on { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
                    )
                    .key(format!("row-lava-{i}"))
                    .radius(radius::MD),
                )
                .child(
                    el().key("check")
                        .size(20.0, 20.0)
                        .radius(radius::SM)
                        .center()
                        .fill(if on {
                            oklch(0.45, 0.15, 40.0)
                        } else {
                            oklch(0.17, 0.02, 35.0)
                        })
                        .border(
                            hair,
                            if on {
                                oklch(0.88, 0.12, 60.0)
                            } else {
                                oklch(0.55, 0.07, 45.0).alpha(0.55)
                            },
                        )
                        .child(
                            fx(gfx::CHECK, [if on { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0])
                                .key(format!("check-lava-{i}"))
                                .radius(radius::SM),
                        )
                        .child(
                            fx(gfx::GLOW, [5.0, if on { 0.45 } else { 0.0 }, 0.5, 0.0])
                                .key(format!("check-glow-{i}"))
                                .radius(radius::SM)
                                .effect_bleed(8.0),
                        )
                        .child(
                            canvas(checkmark(
                                20.0,
                                Color::WHITE.alpha(if on { 1.0 } else { 0.0 }),
                            ))
                            .key(format!("check-mark-{i}"))
                            .size(20.0, 20.0)
                            .scale(if on { 1.0 } else { 0.6 })
                            .scale_motion(motion::POP),
                        ),
                )
                .child(
                    col()
                        .grow(1.0)
                        .min_w(0.0)
                        .gap(2.0)
                        .child(text(&target.label).font(type_::TITLE).color(ink))
                        .child(
                            text(&target.detail)
                                .font(type_::LABEL)
                                .color(secondary)
                                .truncate(),
                        )
                        .child(
                            text(target.impact.tag())
                                .font(type_::SECTION)
                                .color(tag_color),
                        ),
                )
                .child(
                    col()
                        .gap(2.0)
                        .items_end()
                        .child(
                            text(scan::human(target.bytes))
                                .font(type_::MONO_CAPTION)
                                .color(ink),
                        )
                        .child(
                            text(if target.impact.view_only() {
                                "VIEW ONLY"
                            } else if c.undoable {
                                target.kind.label()
                            } else {
                                "NO UNDO"
                            })
                            .font(type_::SECTION)
                            .color(badge),
                        ),
                )
                .child(
                    el().key(format!("row-bar-{i}"))
                        .h(3.0)
                        .w(bar_w)
                        .radius(2.0)
                        .fill(if on {
                            oklch(0.78, 0.14, 155.0)
                        } else {
                            secondary.alpha(0.5)
                        }),
                ),
        );
    }

    let undo = if s.session.quarantine().is_empty() {
        el().key("undo-off").h(0.0)
    } else {
        el().key("undo")
            .h(36.0)
            .px(space::S3)
            .radius(radius::MD)
            .fill(tok::accent(4, true).alpha(0.22))
            .center()
            .on_press(undo_press)
            .child(
                text("Undo")
                    .font(type_::LABEL)
                    .color(oklch(0.85, 0.14, 155.0)),
            )
    };

    let mut activity = col().key("history").gap(2.0);
    activity = activity.child(
        text("Recent activity")
            .font(type_::SECTION)
            .color(secondary),
    );
    for (secs, line) in s.session.history().iter().take(5) {
        activity = activity.child(
            text(format!("{secs} · {line}"))
                .font(type_::MONO_CAPTION)
                .color(secondary.alpha(0.8))
                .truncate(),
        );
    }

    col()
        .key("clean")
        .grow(1.0)
        .min_h(0.0)
        .gap(space::S3)
        .child(
            col()
                .gap(2.0)
                .child(
                    text("Safe to clean")
                        .key("title")
                        .font(type_::HEADING)
                        .color(ink),
                )
                .child(
                    text("Clean allow-listed items. Other large or hidden items are view only. Held bytes return on restart.")
                        .font(type_::BODY)
                        .color(secondary),
                )
                .child(
                    text("Home scan is bounded to 30,000 entries and depth 6.")
                        .font(type_::LABEL)
                        .color(secondary.alpha(0.8)),
                ),
        )
        .child(
            row()
                .key("summary")
                .h(64.0)
                .px(space::S3)
                .gap(space::S3)
                .center_y()
                .radius(radius::LG)
                .fill(card)
                .border(hair, separator)
                .child(
                    fx(gfx::MESH, [0.0, 0.0, 0.0, 0.0])
                        .key("summary-mesh")
                        .radius(radius::LG),
                )
                .child(summary_fx)
                .child(
                    col()
                        .grow(1.0)
                        .gap(2.0)
                        .child(
                            text(format!("{n} selected · {}", scan::human(chosen)))
                                .font(type_::TITLE)
                                .color(ink),
                        )
                        .child(
                            text(format!(
                                "held in quarantine {} · freed {}",
                                scan::human(s.session.held_bytes()),
                                scan::human(s.session.freed_bytes())
                            ))
                            .key("held-clean")
                            .font(type_::LABEL)
                            .color(secondary),
                        )
                        .child(
                            text(if armed {
                                "Press again to remove these from disk."
                            } else {
                                "Review the list, then clean."
                            })
                            .font(type_::LABEL)
                            .color(if armed {
                                oklch(0.8, 0.15, 40.0)
                            } else {
                                secondary
                            }),
                        ),
                )
                .child(undo)
                .child(
                    el().key("clean-button")
                        .h(36.0)
                        .px(space::S6)
                        .radius(radius::MD)
                        .fill(btn_color)
                        .center()
                        .on_press(clean_press)
                        .child(text(label).font(type_::TITLE).color(Color::WHITE)),
                ),
        )
        .child(match &s.notice {
            Some(msg) => text(msg)
                .key("notice")
                .font(type_::LABEL)
                .color(oklch(0.8, 0.14, 155.0)),
            None => el().key("notice").h(0.0),
        })
        .child(
            col()
                .key("list-viewport")
                .grow(1.0)
                .min_h(0.0)
                .scroll(ScrollSpec {
                    stick_bottom: false,
                })
                .gap(space::S2)
                .child(list),
        )
        .child(activity)
}

fn clean_press(s: &mut State) {
    if !s.session.armed() {
        if !s.session.arm() {
            s.notice = Some("Select something first.".into());
        }
        return;
    }
    let report = s.session.clean();
    let mut msg = format!(
        "Cleaned {} path items and {} tools, skipped {}.",
        report.path_items, report.tool_items, report.failed
    );
    if !s.session.quarantine().is_empty() {
        msg.push_str(" Undo lasts until you close cleanX.");
    }
    if let Some(cmd) = report.retry {
        msg.push_str(&format!(" If asked, run: {cmd}."));
    }
    s.notice = Some(msg);
}

fn undo_press(s: &mut State) {
    let report = s.session.undo();
    let mut msg = format!("Restored {}, skipped {}.", report.restored, report.failed);
    if let Some(why) = report.reason {
        msg.push_str(&format!(" Reason: {why}."));
    }
    s.notice = Some(msg);
}

fn main() {
    let app = rune::App::new(AppSpec::<State> {
        init,
        poll: Some(session::app_poll),
        scenes: &[
            ("isolated", |s| s.isolate = true),
            ("clean", |s| s.tab = Tab::Clean),
            ("confirm", |s| {
                s.tab = Tab::Clean;
                s.session.arm_for_scene();
            }),
            ("burst", |s| s.session.set_burst_for_scene()),
            ("burst-control", |_| {}),
            ("undo", |s| {
                s.tab = Tab::Clean;
                s.session.seed_undo_for_scene();
            }),
            ("empty", |s| {
                s.tab = Tab::Clean;
                s.session.clear_candidates();
            }),
            ("low", |s| s.session.set_low_volume()),
        ],
        ..AppSpec::new("cleanX", (760.0, 600.0), State::default, view)
    })
    .compositor(gfx::Gfx::default());
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Env, Runtime};

    fn fake_target() -> scan::CleanTarget {
        scan::CleanTarget {
            path: std::path::PathBuf::from("/home/test/.cache"),
            label: "App cache".into(),
            detail: "test".into(),
            kind: scan::Kind::Cache,
            impact: scan::Impact::Safe,
            bytes: 4096,
            tool: None,
            default_on: true,
        }
    }

    fn fake_view_target() -> scan::CleanTarget {
        scan::CleanTarget {
            path: std::path::PathBuf::from("/home/test/.config"),
            label: "Config".into(),
            detail: "view only".into(),
            kind: scan::Kind::Meta,
            impact: scan::Impact::HandsOff,
            bytes: 1024,
            tool: None,
            default_on: true,
        }
    }

    #[test]
    fn tabs_switch_the_main_pane() {
        let mut rt = Runtime::new(State::default(), view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("space").is_some());
        let (x, y) = rt.find("tab-clean").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.tab, Tab::Clean);
        assert!(rt.find("clean-list").is_some());
    }

    #[test]
    fn clicking_the_orb_opens_clean() {
        let mut rt = Runtime::new(State::default(), view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let (x, y) = rt.find("orb").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert_eq!(rt.state.tab, Tab::Clean);
    }

    #[test]
    fn scan_now_restarts_the_sunburst() {
        let mut rt = Runtime::new(State::default(), view, (760.0, 600.0), Env::new(true, 1.0));
        rt.poll = Some(session::app_poll);
        rt.frame(0.0);
        assert_eq!(rt.state.session.scan_progress(), 1.0);
        let (x, y) = rt.find("scan-now").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(rt.state.session.scan_progress() < 1.0);
        assert!(rt.find("sunburst").is_some());
    }

    #[test]
    fn effects_decorate_the_orb_and_arm_the_clean_bar() {
        let mut rt = Runtime::new(State::default(), view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("orb-lava").is_some());
        assert!(rt.find("orb-glow").is_some());
        assert!(rt.find("sunburst").is_some());
        assert!(rt.find("reclaim-mesh").is_some());

        let mut state = State::default();
        state.tab = Tab::Clean;
        state.session.arm_for_scene();
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("clean-fire").is_some());
    }

    #[test]
    fn a_burst_adds_a_confetti_slot() {
        let mut state = State::default();
        state.session.set_burst_for_scene();
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("confetti").is_some());
    }

    #[test]
    fn a_chosen_row_uses_the_lava_checkbox() {
        let mut state = State::default();
        state.tab = Tab::Clean;
        state.session.set_candidates(vec![fake_target()]);
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("check-lava-0").is_some());
        assert!(rt.find("check-mark-0").is_some());
    }

    #[test]
    fn clicking_a_row_toggles_its_selection() {
        let mut state = State::default();
        state.tab = Tab::Clean;
        state.session.set_candidates(vec![fake_target()]);
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let (x, y) = rt.find("row-0").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(!rt.state.session.candidates()[0].chosen);
    }

    #[test]
    fn a_view_only_row_stays_unselected() {
        let mut state = State::default();
        state.tab = Tab::Clean;
        state.session.set_candidates(vec![fake_view_target()]);
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("row-bar-0").is_some());
        let (x, y) = rt.find("row-0").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(!rt.state.session.candidates()[0].chosen);
        assert!(!rt.state.session.armed());
    }

    #[test]
    fn held_bytes_have_their_own_label() {
        let mut state = State::default();
        state.tab = Tab::Clean;
        state.session.seed_undo_for_scene();
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("held-clean").is_some());
    }

    #[test]
    fn space_shows_a_reclaim_breakdown() {
        let mut state = State::default();
        state.session.set_candidates(vec![fake_target()]);
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        assert!(rt.find("by-kind").is_some());
        assert!(rt.find("kind-chips").is_some());
        assert!(rt.find("disks").is_some());
    }

    #[test]
    fn a_zero_byte_selection_can_be_armed() {
        let mut target = fake_target();
        target.bytes = 0;
        let mut state = State::default();
        state.tab = Tab::Clean;
        state.session.set_candidates(vec![target]);
        let mut rt = Runtime::new(state, view, (760.0, 600.0), Env::new(true, 1.0));
        rt.frame(0.0);
        let (x, y) = rt.find("clean-button").unwrap();
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(0.0);
        assert!(rt.state.session.armed());
    }
}
