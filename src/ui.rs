use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;
use crate::fmt;

const LABEL: usize = 7;
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

fn meter<'a>(label: &'a str, percent: Option<u16>, right: String, width: usize) -> Line<'a> {
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
        Span::styled(format!(" {key:<LABEL$}"), KEY),
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
        Span::styled(format!(" {:<LABEL$}", "runner"), KEY),
        Span::styled(state, style),
    ]);
    let up = app.uptime.secs.map(fmt::hms).unwrap_or_else(|| "--".into());
    Paragraph::new(vec![
        s,
        kv("jobs", app.jobs.running.to_string()),
        kv("uptime", up),
    ])
}

fn meters(app: &App, width: u16) -> Paragraph<'_> {
    let w = bar_width(width);
    let mut out = Vec::with_capacity(5);

    let load = app
        .cpu
        .load_avg
        .map(|l| {
            format!(
                "load {} {} {}",
                fmt::centi(l.one),
                fmt::centi(l.five),
                fmt::centi(l.fifteen)
            )
        })
        .unwrap_or_default();
    let mhz = app
        .cpu
        .mhz
        .map(|m| format!("{m} MHz  "))
        .unwrap_or_default();
    out.push(meter("cpu", app.cpu.percent, format!("{mhz}{load}"), w));

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

pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let [top, cap, met, net, foot] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Length(6),
        Constraint::Length(7),
        Constraint::Length(4),
        Constraint::Min(1),
    ])
    .areas(area);

    frame.render_widget(header(app).block(Block::bordered().border_style(DIM)), top);

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
        meters(app, met.width).block(
            Block::bordered()
                .border_style(DIM)
                .title(Span::styled(" system ", KEY)),
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
    fn heat_escalates_with_utilization() {
        assert_eq!(heat(10).fg, Some(Color::Green));
        assert_eq!(heat(70).fg, Some(Color::Yellow));
        assert_eq!(heat(99).fg, Some(Color::Red));
    }
}
