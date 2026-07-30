use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Padding, Paragraph};

use crate::app::App;
use crate::fmt;

const LABEL: usize = 10;
const KEYW: usize = 10;
const DIM: Style = Style::new().fg(Color::DarkGray);
const KEY: Style = Style::new().fg(Color::Gray);

fn heat(percent: u16) -> Style {
    let c = if percent >= 85 {
        Color::Red
    } else if percent >= 60 {
        Color::Yellow
    } else {
        Color::Green
    };
    Style::new().fg(c)
}

pub fn inner_width(total: u16) -> u16 {
    total.saturating_sub(4)
}

pub const OVERHEAD: u16 = LABEL as u16 + 9;

pub fn bar_width(inner: u16, reserved: u16) -> usize {
    inner.saturating_sub(OVERHEAD + reserved).clamp(8, 32) as usize
}

pub fn full_bar_width(inner: u16) -> usize {
    inner.saturating_sub(OVERHEAD).max(8) as usize
}

fn pad_block(title: &str) -> Block<'_> {
    Block::bordered()
        .border_style(DIM)
        .padding(Padding::horizontal(1))
        .title(Line::from(Span::styled(title, KEY)).right_aligned())
}

fn meter(label: &str, percent: Option<u16>, right: String, width: usize) -> Line<'static> {
    meter_owned(label.to_string(), percent, right, width)
}

fn meter_owned(label: String, percent: Option<u16>, right: String, width: usize) -> Line<'static> {
    let mut spans = vec![Span::styled(format!("{label:<LABEL$}"), KEY)];
    match percent {
        Some(p) => {
            spans.push(Span::styled("[", DIM));
            spans.push(Span::styled(fmt::bar(p, width), heat(p)));
            spans.push(Span::styled("] ", DIM));
            spans.push(Span::raw(format!("{p:>3}%  ")));
        }
        None => {
            spans.push(Span::styled("[", DIM));
            spans.push(Span::styled(fmt::bar(0, width), DIM));
            spans.push(Span::styled("]   --  ", DIM));
        }
    }
    spans.push(Span::styled(right, DIM));
    Line::from(spans)
}

fn centi_opt(v: Option<u64>) -> String {
    v.map(fmt::centi).unwrap_or_else(|| "--".into())
}

fn kv<'a>(key: &'a str, value: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{key:<KEYW$}"), KEY),
        Span::raw(value),
    ])
}

fn header(app: &App) -> Paragraph<'_> {
    let h = &app.host;
    let title = Line::from(vec![
        Span::styled(
            " citop ",
            Style::new()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            h.nodename.as_str(),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]);
    let os = Line::from(vec![Span::styled(
        format!("{} {} {}", h.os, h.kernel, h.arch),
        DIM,
    )]);
    let model = Line::from(vec![Span::styled(h.model.as_str(), DIM)]);
    Paragraph::new(vec![title, os, model])
}

fn capacity(app: &App) -> Paragraph<'_> {
    let h = &app.host;
    let freq = match (h.mhz_min, h.mhz_max) {
        (Some(a), Some(b)) => format!("{} x {}-{} MHz", h.cores, a, b),
        _ => format!("{} cores", h.cores),
    };
    let clock = app
        .cpu
        .mhz
        .map(|m| format!("{m} MHz"))
        .unwrap_or_else(|| "--".into());
    let load = app
        .cpu
        .load_avg
        .map(|l| {
            format!(
                "{} {} {}",
                fmt::centi(l.one),
                fmt::centi(l.five),
                fmt::centi(l.fifteen)
            )
        })
        .unwrap_or_else(|| "--".into());
    let psi = format!(
        "cpu {} mem {} io {}",
        centi_opt(app.psi.cpu_centi),
        centi_opt(app.psi.mem_centi),
        centi_opt(app.psi.io_centi)
    );
    Paragraph::new(vec![
        kv("cpu", freq),
        kv("clock", clock),
        kv("load", load),
        kv("psi", psi),
    ])
}

fn runner(app: &App) -> Paragraph<'_> {
    let state = app.jobs.state();
    let style = match state {
        "busy" => Style::new().fg(Color::Yellow),
        "listening" => Style::new().fg(Color::Green),
        _ => Style::new().fg(Color::Red),
    };
    let s = Line::from(vec![
        Span::styled(format!("{:<KEYW$}", "runner"), KEY),
        Span::styled(state, style),
    ]);
    let up = app.uptime.secs.map(fmt::hms).unwrap_or_else(|| "--".into());
    let last = app.jobs.last_job.clone().unwrap_or_else(|| "never".into());
    let res = match app.jobs.res {
        Some(r) => format!("{}  {} pids", fmt::pair(r.mem, r.mem_peak), r.pids),
        None => "--".into(),
    };
    Paragraph::new(vec![
        s,
        kv("jobs", app.jobs.running.to_string()),
        kv("uptime", up),
        kv("last job", last),
        kv("res", res),
    ])
}

fn cpu_section(app: &App, inner: u16, cpu_rows: usize) -> Paragraph<'_> {
    let full = full_bar_width(inner);
    let mut out = Vec::with_capacity(cpu_rows);
    if cpu_rows > 1 {
        for i in 0..cpu_rows {
            let p = app.cpu.per_core.get(i).copied().flatten();
            out.push(meter_owned(format!("cpu_{i}"), p, String::new(), full));
        }
    } else {
        out.push(meter("cpu", app.cpu.percent, String::new(), full));
    }
    Paragraph::new(out)
}

fn temp_section(app: &App, inner: u16) -> Paragraph<'_> {
    let w = bar_width(inner, 8);
    let right = match app.therm.milli_c {
        Some(t) => format!("{} C", fmt::milli(t)),
        None => "N/A".into(),
    };
    let pct = app.therm.milli_c.map(|_| app.therm.percent_of_range());
    let fan_style = match app.fan.on {
        Some(true) => Style::new().fg(Color::Green),
        Some(false) => DIM,
        None => DIM,
    };
    Paragraph::new(vec![
        meter("cpu-temp", pct, right, w),
        Line::from(vec![
            Span::styled(format!("{:<KEYW$}", "fan"), KEY),
            Span::styled(app.fan.label(), fan_style),
        ]),
        kv("fan-speed", app.fan.rpm_label()),
        Line::from(vec![
            Span::styled(format!("{:<KEYW$}", "throttle"), KEY),
            Span::styled(
                app.throttle.label(),
                if app.throttle.active() {
                    Style::new().fg(Color::Red)
                } else if app.throttle.ever() {
                    Style::new().fg(Color::Yellow)
                } else {
                    DIM
                },
            ),
        ]),
    ])
}

fn memory_section(app: &App, inner: u16) -> Paragraph<'_> {
    let w = bar_width(inner, 34);
    let mut out = Vec::with_capacity(3);
    match app.mem.info {
        Some(m) => {
            out.push(meter(
                "ram",
                Some(fmt::pct(m.used(), m.total)),
                format!(
                    "{} / {}  cache {}",
                    fmt::bytes(m.used()),
                    fmt::bytes(m.total),
                    fmt::bytes(m.buffcache)
                ),
                w,
            ));
            let sp = if m.swap_total > 0 {
                Some(fmt::pct(m.swap_used(), m.swap_total))
            } else {
                None
            };
            out.push(meter(
                "swap",
                sp,
                format!(
                    "{} / {}",
                    fmt::bytes(m.swap_used()),
                    fmt::bytes(m.swap_total)
                ),
                w,
            ));
        }
        None => {
            out.push(meter("ram", None, "unavailable".into(), w));
            out.push(meter("swap", None, "unavailable".into(), w));
        }
    }
    match app.mem.disk {
        Some(d) => out.push(meter(
            "disk",
            Some(d.percent()),
            format!(
                "{} / {}  free {}",
                fmt::bytes(d.used),
                fmt::bytes(d.total),
                fmt::bytes(d.avail)
            ),
            w,
        )),
        None => out.push(meter("disk", None, "unavailable".into(), w)),
    }
    let io_right = format!(
        "{}  rd {}  wr {}",
        app.diskio.device(),
        app.diskio
            .read_bps
            .map(fmt::rate)
            .unwrap_or_else(|| "--".into()),
        app.diskio
            .write_bps
            .map(fmt::rate)
            .unwrap_or_else(|| "--".into())
    );
    out.push(meter("disk-io", app.diskio.util, io_right, w));
    Paragraph::new(out)
}

fn iface_line<'a>(slot: &'a str, i: Option<&'a crate::net::Iface>) -> Line<'a> {
    let Some(i) = i else {
        return Line::from(vec![
            Span::styled(format!("{slot:<LABEL$}"), KEY),
            Span::styled("not present", DIM),
        ]);
    };
    let link = match i.speed {
        Some(s) => format!("{s} Mb"),
        None => "--".into(),
    };
    let state_style = if i.state == "up" {
        Style::new().fg(Color::Green)
    } else {
        Style::new().fg(Color::Red)
    };
    let rx = i.rx_rate.map(fmt::rate).unwrap_or_else(|| "--".into());
    let tx = i.tx_rate.map(fmt::rate).unwrap_or_else(|| "--".into());
    Line::from(vec![
        Span::styled(format!("{:<LABEL$}", i.name), KEY),
        Span::styled(format!("{:<5}", i.state), state_style),
        Span::styled(format!("{link:>7}  "), DIM),
        Span::styled("rx ", DIM),
        Span::raw(format!("{rx:>11}  ")),
        Span::styled("tx ", DIM),
        Span::raw(format!("{tx:>11}")),
        Span::styled("  err ", DIM),
        Span::styled(
            i.errs.to_string(),
            if i.errs > 0 {
                Style::new().fg(Color::Red)
            } else {
                DIM
            },
        ),
        Span::styled(" drop ", DIM),
        Span::styled(
            i.drops.to_string(),
            if i.drops > 0 {
                Style::new().fg(Color::Yellow)
            } else {
                DIM
            },
        ),
    ])
}

fn network(app: &App) -> Paragraph<'_> {
    Paragraph::new(vec![
        iface_line("eth", app.net.eth.as_ref()),
        iface_line("wlan", app.net.wlan.as_ref()),
    ])
}

pub const CAPACITY_ROWS: u16 = 4;
pub const RUNNER_ROWS: u16 = 5;
pub const TEMP_ROWS: usize = 4;
pub const MEMORY_ROWS: u16 = 4;
pub const NETWORK_ROWS: u16 = 2;
pub const HEADER_FULL: u16 = 3;
pub const HEADER_MIN: u16 = 1;

pub fn fixed_rows() -> u16 {
    let row1 = CAPACITY_ROWS.max(RUNNER_ROWS) + 2;
    let row3 = MEMORY_ROWS + 2;
    let row4 = NETWORK_ROWS + 2;
    row1 + row3 + row4
}

pub fn cpu_rows_for(height: u16, cores: usize) -> usize {
    if cores <= 1 {
        return 1;
    }
    let need = |n: usize| HEADER_MIN + fixed_rows() + n.max(TEMP_ROWS) as u16 + 2;
    if height >= need(cores) { cores } else { 1 }
}

pub fn header_rows_for(height: u16, cpu_rows: usize) -> u16 {
    let body = fixed_rows() + cpu_rows.max(TEMP_ROWS) as u16 + 2;
    if height >= body + HEADER_FULL {
        HEADER_FULL
    } else {
        HEADER_MIN
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let cores = app.cpu.per_core.len().max(app.host.cores);
    let cpu_rows = cpu_rows_for(area.height, cores);
    let mid = cpu_rows.max(TEMP_ROWS) as u16 + 2;

    let [top, row1, row2, row3, row4, foot] = Layout::vertical([
        Constraint::Length(header_rows_for(area.height, cpu_rows)),
        Constraint::Length(CAPACITY_ROWS.max(RUNNER_ROWS) + 2),
        Constraint::Length(mid),
        Constraint::Length(MEMORY_ROWS + 2),
        Constraint::Length(NETWORK_ROWS + 2),
        Constraint::Min(0),
    ])
    .areas(area);

    frame.render_widget(
        header(app).block(Block::new().padding(Padding::horizontal(2))),
        top,
    );

    let halves = [Constraint::Percentage(50), Constraint::Percentage(50)];
    let [cap_a, run_a] = Layout::horizontal(halves).areas(row1);
    frame.render_widget(capacity(app).block(pad_block(" capacity ")), cap_a);
    frame.render_widget(runner(app).block(pad_block(" runner ")), run_a);

    let [cpu_a, temp_a] = Layout::horizontal(halves).areas(row2);
    frame.render_widget(
        cpu_section(app, inner_width(cpu_a.width), cpu_rows).block(pad_block(" cpu ")),
        cpu_a,
    );
    frame.render_widget(
        temp_section(app, inner_width(temp_a.width)).block(pad_block(" temp ")),
        temp_a,
    );

    frame.render_widget(
        memory_section(app, inner_width(row3.width)).block(pad_block(" memory ")),
        row3,
    );
    frame.render_widget(network(app).block(pad_block(" networking ")), row4);
    footer(frame, foot);
}

fn footer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let line = Line::from(vec![
        Span::styled("q", KEY),
        Span::styled(" quit   ", DIM),
        Span::styled("r", KEY),
        Span::styled(" refresh", DIM),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(Block::new().padding(Padding::horizontal(2))),
        area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_KEYS: [&str; 15] = [
        "cpu",
        "clock",
        "load",
        "psi",
        "runner",
        "jobs",
        "uptime",
        "last job",
        "res",
        "cpu-temp",
        "fan",
        "fan-speed",
        "throttle",
        "disk-io",
        "wlan0",
    ];

    #[test]
    fn bar_width_stays_within_bounds_at_any_terminal_size() {
        assert_eq!(bar_width(0, 34), 8);
        assert_eq!(bar_width(52, 34), 8);
        assert_eq!(bar_width(76, 34), 23);
        assert_eq!(bar_width(300, 34), 32);
    }

    #[test]
    fn core_bars_consume_the_whole_inner_width() {
        for total in [80u16, 96, 120, 200] {
            let inner = inner_width(total);
            let line = meter_owned(
                "cpu_0".into(),
                Some(42),
                String::new(),
                full_bar_width(inner),
            );
            assert_eq!(
                line.width(),
                inner as usize,
                "core row does not fill inner width at total={total}"
            );
        }
    }

    #[test]
    fn a_core_row_without_a_sample_still_fills_the_width() {
        let inner = inner_width(80);
        let line = meter_owned("cpu_1".into(), None, String::new(), full_bar_width(inner));
        assert_eq!(line.width(), inner as usize);
    }

    #[test]
    fn labelled_rows_fit_an_eighty_column_terminal() {
        let inner = inner_width(80);
        let line = meter(
            "ram",
            Some(42),
            "524.1 MiB / 3.6 GiB  cache 3.0 GiB".into(),
            bar_width(inner, 34),
        );
        assert!(
            line.width() <= inner as usize,
            "ram row width {} exceeds inner {inner}",
            line.width()
        );
    }

    #[test]
    fn meter_renders_placeholders_before_the_first_delta() {
        let line = meter("cpu", None, String::new(), 8);
        assert!(line.to_string().contains("--"));
    }

    #[test]
    fn per_core_rows_are_shown_when_the_terminal_is_tall_enough() {
        assert_eq!(cpu_rows_for(24, 4), 4);
        assert_eq!(cpu_rows_for(30, 4), 4);
        assert_eq!(cpu_rows_for(28, 8), 8);
    }

    #[test]
    fn per_core_rows_collapse_to_aggregate_when_too_short() {
        assert_eq!(cpu_rows_for(10, 4), 1);
        assert_eq!(cpu_rows_for(27, 8), 1);
    }

    #[test]
    fn the_whole_layout_fits_eighty_by_twenty_four() {
        let cpu_rows = cpu_rows_for(24, 4);
        assert_eq!(cpu_rows, 4, "all four cores must still be shown at 80x24");
        let total =
            header_rows_for(24, cpu_rows) + fixed_rows() + cpu_rows.max(TEMP_ROWS) as u16 + 2;
        assert!(total <= 24, "layout needs {total} rows, only 24 available");
    }

    #[test]
    fn the_header_expands_when_there_is_room() {
        assert_eq!(header_rows_for(24, 4), HEADER_MIN);
        assert_eq!(header_rows_for(26, 4), HEADER_FULL);
        assert_eq!(header_rows_for(40, 4), HEADER_FULL);
    }

    #[test]
    fn a_single_core_machine_never_disaggregates() {
        assert_eq!(cpu_rows_for(60, 1), 1);
        assert_eq!(cpu_rows_for(60, 0), 1);
    }

    #[test]
    fn core_labels_are_zero_indexed_and_padded() {
        let line = meter_owned("cpu_0".into(), Some(50), String::new(), 8);
        assert!(line.to_string().starts_with("cpu_0  "));
    }

    #[test]
    fn rows_carry_no_leading_space_since_padding_supplies_it() {
        assert!(!kv("cpu", "x".into()).to_string().starts_with(' '));
        assert!(
            !meter_owned("cpu_0".into(), Some(1), String::new(), 8)
                .to_string()
                .starts_with(' ')
        );
    }

    #[test]
    fn kv_keeps_a_separator_after_the_longest_key() {
        for key in ALL_KEYS {
            let s = kv(key, "VALUE".into()).to_string();
            assert!(
                s.contains(&format!("{key} ")),
                "key {key} has no separator before its value: {s:?}"
            );
            assert!(s.ends_with("VALUE"));
        }
    }

    #[test]
    fn kv_columns_align_across_both_panels() {
        let a = kv("cpu", "X".into()).to_string();
        let b = kv("last job", "X".into()).to_string();
        assert_eq!(a.find('X'), b.find('X'));
    }

    #[test]
    fn every_meter_label_keeps_a_separator_before_its_bar() {
        for key in ALL_KEYS {
            let s = meter_owned(key.to_string(), Some(1), String::new(), 8).to_string();
            assert!(
                s.starts_with(&format!("{key} ")),
                "meter label {key} touches its bar: {s:?}"
            );
        }
    }

    #[test]
    fn heat_escalates_with_utilization() {
        assert_eq!(heat(10).fg, Some(Color::Green));
        assert_eq!(heat(70).fg, Some(Color::Yellow));
        assert_eq!(heat(99).fg, Some(Color::Red));
    }
}
