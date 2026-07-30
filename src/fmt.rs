pub const UNITS: [&str; 7] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

pub fn bytes(n: u64) -> String {
    let mut i = 0usize;
    let mut v = n;
    while v >= 1024 && i < UNITS.len() - 1 {
        v /= 1024;
        i += 1;
    }
    if i == 0 {
        return format!("{n} B");
    }
    let scale = 1u64 << (10 * i as u32);
    let whole = n / scale;
    let tenth = (n % scale) * 10 / scale;
    format!("{whole}.{tenth} {}", UNITS[i])
}

pub fn rate(bps: u64) -> String {
    format!("{}/s", bytes(bps))
}

pub fn pct(num: u64, den: u64) -> u16 {
    if den == 0 {
        return 0;
    }
    let p = num.saturating_mul(100) / den;
    if p > 100 { 100 } else { p as u16 }
}

pub fn milli(v: i64) -> String {
    let neg = v < 0;
    let a = v.unsigned_abs();
    let whole = a / 1000;
    let tenth = (a % 1000) / 100;
    if neg {
        format!("-{whole}.{tenth}")
    } else {
        format!("{whole}.{tenth}")
    }
}

pub fn centi(v: u64) -> String {
    format!("{}.{:02}", v / 100, v % 100)
}

pub fn hms(secs: u64) -> String {
    let d = secs / 86400;
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    if d > 0 {
        format!("{d}d {h}h {m}m")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{m}m")
    }
}

pub fn bar(percent: u16, width: usize) -> String {
    let p = if percent > 100 { 100 } else { percent } as usize;
    let filled = p * width / 100;
    let mut s = String::with_capacity(width * 3);
    for i in 0..width {
        s.push(if i < filled { '\u{2588}' } else { '\u{2591}' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_scales_and_keeps_one_decimal() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(1536), "1.5 KiB");
        assert_eq!(bytes(3_967_889_408), "3.6 GiB");
        assert_eq!(bytes(1u64 << 40), "1.0 TiB");
        assert_eq!(bytes(u64::MAX), "15.9 EiB");
    }

    #[test]
    fn rate_appends_per_second() {
        assert_eq!(rate(2048), "2.0 KiB/s");
    }

    #[test]
    fn pct_clamps_and_guards_zero_denominator() {
        assert_eq!(pct(0, 0), 0);
        assert_eq!(pct(5, 0), 0);
        assert_eq!(pct(1, 2), 50);
        assert_eq!(pct(3, 2), 100);
        assert_eq!(pct(u64::MAX, 1), 100);
    }

    #[test]
    fn milli_renders_one_decimal_without_float() {
        assert_eq!(milli(50634), "50.6");
        assert_eq!(milli(48199), "48.1");
        assert_eq!(milli(0), "0.0");
        assert_eq!(milli(-1500), "-1.5");
    }

    #[test]
    fn centi_pads_two_decimals() {
        assert_eq!(centi(8), "0.08");
        assert_eq!(centi(234), "2.34");
        assert_eq!(centi(100), "1.00");
    }

    #[test]
    fn hms_picks_coarsest_useful_unit() {
        assert_eq!(hms(59), "0m");
        assert_eq!(hms(600), "10m");
        assert_eq!(hms(3720), "1h 2m");
        assert_eq!(hms(90061), "1d 1h 1m");
    }

    #[test]
    fn bar_fills_proportionally_and_clamps() {
        assert_eq!(bar(0, 4).chars().filter(|c| *c == '\u{2588}').count(), 0);
        assert_eq!(bar(50, 4).chars().filter(|c| *c == '\u{2588}').count(), 2);
        assert_eq!(bar(100, 4).chars().filter(|c| *c == '\u{2588}').count(), 4);
        assert_eq!(bar(250, 4).chars().filter(|c| *c == '\u{2588}').count(), 4);
        assert_eq!(bar(37, 10).chars().count(), 10);
    }
}
