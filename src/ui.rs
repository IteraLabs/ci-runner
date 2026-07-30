use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::fmt;

const LABEL: usize = 7;
const KEYW: usize = 9;
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

fn bar_width(total: u16) -> usize {
    let usable = total.saturating_sub(55);
    usable.clamp(8, 32) as usize
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

fn kv<'a>(key: &'a str, value: String) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!(" {key:<KEYW$}"), KEY),
        Span::raw(value),
    ])
}

fn header(app: &App) -> Paragraph<'_> {
    let h = &app.host;
    let title = Line::from(vec![
        Span::styled(
            " citop ",
            Style::new().fg(Color::Black).bg(Color::Cyan).bold(),
        ),
        Span::raw(" "),
        Span::styled(
            h.nodename.as_str(),
            Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
    ]);
    let os = Line::from(vec![Span::styled(
        format!(" {} {} {}", h.os, h.kernel, h.arch),
        DIM,
    )]);
    let model = Line::from(vec![Span::styled(format!(" {}", h.model), DIM)]);
    Paragraph::new(vec![title, os, model])
}

fn capacity(app: &App) -> Paragraph<'_> {
    let h = &app.host;
    let freq = match (h.mhz_min, h.mhz_max) {
        (Some(a), Some(b)) => format!("{} x {}-{} MHz", h.cores, a, b),
        _ => format!("{} cores", h.cores),
    };
    let ram = app
        .mem
        .info
        .map(|m| fmt::bytes(m.total))
        .unwrap_or_else(|| "--".into());
    let disk = app
        .mem
        .disk
        .map(|d| fmt::bytes(d.total))
        .unwrap_or_else(|| "--".into());
    let tasks = app
        .cpu
        .load_avg
        .map(|l| format!("{} runnable / {}", l.runnable, l.threads))
        .unwrap_or_else(|| "--".into());
    Paragraph::new(vec![
        kv("cpu", freq),
        kv("ram", ram),
        kv("disk", disk),
        kv("tasks", tasks),
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
        Span::styled(format!(" {:<KEYW$}", "runner"), KEY),
        Span::styled(state, style),
    ]);
    let up = app.uptime.secs.map(fmt::hms).unwrap_or_else(|| "--".into());
    let last = app
        .jobs
        .last_job
        .clone()
        .unwrap_or_else(|| "never".into());
    Paragraph::new(vec![
        s,
        kv("jobs", app.jobs.running.to_string()),
        kv("uptime", up),
        kv("last job", last),
    ])
}

pub fn cpu_summary(app: &App) -> String {
    let agg = app
        .cpu
        .percent
        .map(|p| format!("cpu {p}%"))
        .unwrap_or_else(|| "cpu --".into());
    let mhz = app
        .cpu
        .mhz
        .map(|m| format!("  {m} MHz"))
        .unwrap_or_default();
    let load = app
        .cpu
        .load_avg
        .map(|l| {
            format!(
                "  load {} {} {}",
                fmt::centi(l.one),
                fmt::centi(l.five),
                fmt::centi(l.fifteen)
            )
        })
        .unwrap_or_default();
    format!(" {agg}{mhz}{load} ")
}

fn meters(app: &App, width: u16, cpu_rows: usize) -> Paragraph<'_> {
    let w = bar_width(width);
    let mut out = Vec::with_capacity(cpu_rows + 4);

    if cpu_rows > 1 {
        for i in 0..cpu_rows {
            let p = app.cpu.per_core.get(i).copied().flatten();
            out.push(meter_owned(format!("cpu_{i}"), p, String::new(), w));
        }
    } else {
        out.push(meter("cpu", app.cpu.percent, String::new(), w));
    }

    let tright = match (app.therm.milli_c, app.therm.trip_milli_c) {
        (Some(t), Some(trip)) => format!("{} C  fan trip {} C", fmt::milli(t), fmt::milli(trip)),
        (Some(t), None) => format!("{} C", fmt::milli(t)),
        _ => "sensor unavailable".into(),
    };
    let tpct = app.therm.milli_c.map(|_| app.therm.percent_of_range());
    out.push(meter("temp", tpct, tright, w));

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

    Paragraph::new(out)
}

fn iface_line<'a>(slot: &'a str, i: Option<&'a crate::net::Iface>) -> Line<'a> {
    let Some(i) = i else {
        return Line::from(vec![
            Span::styled(format!(" {slot:<LABEL$}"), KEY),
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
        Span::styled(format!(" {:<LABEL$}", i.name), KEY),
        Span::styled(format!("{:<5}", i.state), state_style),
        Span::styled(format!("{link:>7}  "), DIM),
        Span::styled("rx ", DIM),
        Span::raw(format!("{rx:>12}  ")),
        Span::styled("tx ", DIM),
        Span::raw(format!("{tx:>12}")),
    ])
}

fn network(app: &App) -> Paragraph<'_> {
    Paragraph::new(vec![
        iface_line("eth", app.net.eth.as_ref()),
        iface_line("wlan", app.net.wlan.as_ref()),
    ])
}

pub const FIXED_ROWS: u16 = 20;

pub fn cpu_rows_for(height: u16, cores: usize) -> usize {
    if cores <= 1 {
        return 1;
    }
    if height >= FIXED_ROWS + cores as u16 {
        cores
    } else {
        1
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let cpu_rows = cpu_rows_for(area.height, app.cpu.per_core.len().max(app.host.cores));
    let [top, cap, met, net, foot] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(6),
        Constraint::Length(cpu_rows as u16 + 6),
        Constraint::Length(4),
        Constraint::Min(0),
    ])
    .areas(area);

    frame.render_widget(header(app), top);

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(cap);
    frame.render_widget(
        capacity(app).block(
            Block::bordered()
                .border_style(DIM)
                .title(Span::styled(" capacity ", KEY)),
        ),
        left,
    );
    frame.render_widget(
        runner(app).block(
            Block::bordered()
                .border_style(DIM)
                .title(Span::styled(" runner ", KEY)),
        ),
        right,
    );

    frame.render_widget(
        meters(app, met.width, cpu_rows).block(
            Block::bordered()
                .border_style(DIM)
                .title(Span::styled(cpu_summary(app), KEY)),
        ),
        met,
    );
    frame.render_widget(
        network(app).block(
            Block::bordered()
                .border_style(DIM)
                .title(Span::styled(" network ", KEY)),
        ),
        net,
    );
    footer(frame, foot);
}

fn footer(frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let line = Line::from(vec![
        Span::styled(" q", KEY),
        Span::styled(" quit   ", DIM),
        Span::styled("r", KEY),
        Span::styled(" refresh", DIM),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_width_stays_within_bounds_at_any_terminal_size() {
        assert_eq!(bar_width(0), 8);
        assert_eq!(bar_width(55), 8);
        assert_eq!(bar_width(78), 23);
        assert_eq!(bar_width(300), 32);
    }

    #[test]
    fn meter_line_fits_an_eighty_column_terminal() {
        let w = bar_width(78);
        let line = meter("cpu", Some(42), "1600 MHz  load 0.08 0.02 0.01".into(), w);
        assert!(
            line.width() <= 78,
            "meter width {} exceeds 78",
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
        assert_eq!(cpu_rows_for(23, 4), 1);
        assert_eq!(cpu_rows_for(10, 4), 1);
        assert_eq!(cpu_rows_for(24, 8), 1);
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
    fn kv_keeps_a_separator_after_the_longest_key() {
        for key in ["cpu", "ram", "disk", "tasks", "runner", "uptime", "last job"] {
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
    fn heat_escalates_with_utilization() {
        assert_eq!(heat(10).fg, Some(Color::Green));
        assert_eq!(heat(70).fg, Some(Color::Yellow));
        assert_eq!(heat(99).fg, Some(Color::Red));
    }
}
