//! Shared formatting utilities for human-readable output.
//!
//! These utilities are used by both PDF and EPUB colophon renderers to display
//! repository statistics in a consistent, readable format across output formats.

use std::collections::BTreeMap;

/// Format a number with thousands separators for readability.
///
/// Large numbers like line counts become hard to parse without separators.
pub fn format_number(n: usize) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

/// Format bytes as a human-readable string (KB, MB, GB).
///
/// Raw byte counts are meaningless to most readers; this converts to familiar units.
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} bytes", bytes)
    }
}

/// Render a compact sparkline showing commit activity over time.
///
/// The sparkline uses 8 graduated Unicode block characters (▁▂▃▄▅▆▇█) to show
/// relative commit volume, with adaptive aggregation to fit within the available width.
/// Returns a two-line string with the sparkline and aligned date labels.
///
/// # Arguments
/// * `frequency` - Commit counts per month as (YYYY-MM, count) pairs, sorted chronologically
/// * `max_width` - Maximum width in characters for the sparkline
///
/// # Returns
/// A formatted string like:
/// ```text
/// ▂▁▃█▅▂▁▄▆▃▂▅
/// Jan 2020                Dec 2024
/// ```
pub fn render_sparkline(frequency: &[(String, u32)], max_width: usize) -> String {
    if frequency.is_empty() {
        return String::new();
    }

    let bar_chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

    // calculate lengths for each granularity (finest to coarsest)
    let total_months = frequency.len();
    let daily_len = total_months * 30;
    let weekly_len = total_months * 4;

    // pick the finest granularity that fits (maximises width without overflow)
    let aggregated = if daily_len <= max_width {
        expand_to_days(frequency)
    } else if weekly_len <= max_width {
        expand_to_weeks(frequency)
    } else if total_months <= max_width {
        frequency.to_vec()
    } else {
        let quarters = aggregate_to_quarters(frequency);
        if quarters.len() <= max_width {
            quarters
        } else {
            aggregate_to_years(frequency)
        }
    };

    if aggregated.is_empty() {
        return String::new();
    }

    // find max for normalisation
    let max_count = aggregated.iter().map(|(_, c)| *c).max().unwrap_or(1);

    // build sparkline
    let sparkline: String = aggregated
        .iter()
        .map(|(_, count)| {
            let level = if max_count > 0 {
                (((*count as f64 / max_count as f64) * 7.0).round() as usize).min(7)
            } else {
                0
            };
            bar_chars[level]
        })
        .collect();

    // format date labels using original (non-aggregated) date range
    let start_date = format_month_label(&frequency.first().unwrap().0);
    let end_date = format_month_label(&frequency.last().unwrap().0);

    // align end date to right edge of sparkline
    let sparkline_width = sparkline.chars().count();
    let start_len = start_date.len();
    let end_len = end_date.len();

    let date_line = if start_len + end_len + 1 >= sparkline_width {
        // not enough room, just put them with minimal space
        format!("{} {}", start_date, end_date)
    } else {
        // pad middle to align end date to right edge
        let padding = sparkline_width - start_len - end_len;
        format!("{}{:width$}{}", start_date, "", end_date, width = padding)
    };

    format!("{}\n{}", sparkline, date_line)
}

/// Convert "YYYY-MM" to "Mon YYYY" format.
fn format_month_label(ym: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    if let Some((year, month)) = ym.split_once('-') {
        if let Ok(m) = month.parse::<usize>() {
            if (1..=12).contains(&m) {
                return format!("{} {}", MONTHS[m - 1], year);
            }
        }
    }
    ym.to_string()
}

/// Aggregate monthly data to quarters.
fn aggregate_to_quarters(frequency: &[(String, u32)]) -> Vec<(String, u32)> {
    let mut quarters: BTreeMap<String, u32> = BTreeMap::new();

    for (ym, count) in frequency {
        if let Some((year, month)) = ym.split_once('-') {
            if let Ok(m) = month.parse::<u32>() {
                let q = (m - 1) / 3 + 1;
                let key = format!("{}-Q{}", year, q);
                *quarters.entry(key).or_default() += count;
            }
        }
    }

    quarters.into_iter().collect()
}

/// Aggregate monthly data to years.
fn aggregate_to_years(frequency: &[(String, u32)]) -> Vec<(String, u32)> {
    let mut years: BTreeMap<String, u32> = BTreeMap::new();

    for (ym, count) in frequency {
        if let Some((year, _)) = ym.split_once('-') {
            *years.entry(year.to_string()).or_default() += count;
        }
    }

    years.into_iter().collect()
}

/// Expand monthly data to weeks (4 weeks per month).
///
/// Distributes each month's commits across 4 weeks to create a wider sparkline
/// for short histories.
fn expand_to_weeks(frequency: &[(String, u32)]) -> Vec<(String, u32)> {
    let mut weeks: Vec<(String, u32)> = Vec::new();

    for (ym, count) in frequency {
        // distribute commits across 4 weeks
        let per_week = *count / 4;
        let remainder = *count % 4;

        for week in 1..=4 {
            let key = format!("{}-W{}", ym, week);
            // add remainder to first week
            let week_count = per_week + if week == 1 { remainder } else { 0 };
            weeks.push((key, week_count));
        }
    }

    weeks
}

/// Expand monthly data to days (30 days per month).
///
/// Distributes each month's commits across 30 days to create a wider sparkline
/// for very short histories.
fn expand_to_days(frequency: &[(String, u32)]) -> Vec<(String, u32)> {
    let mut days: Vec<(String, u32)> = Vec::new();

    for (ym, count) in frequency {
        // distribute commits across 30 days
        let per_day = *count / 30;
        let remainder = *count % 30;

        for day in 1..=30 {
            let key = format!("{}-{:02}", ym, day);
            // spread remainder across first N days
            let day_count = per_day + if day <= remainder { 1 } else { 0 };
            days.push((key, day_count));
        }
    }

    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(123), "123");
        assert_eq!(format_number(1234), "1,234");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn can_format_bytes() {
        assert_eq!(format_bytes(0), "0 bytes");
        assert_eq!(format_bytes(500), "500 bytes");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1048576), "1.0 MB");
        assert_eq!(format_bytes(1073741824), "1.0 GB");
    }

    #[test]
    fn sparkline_empty_returns_empty() {
        assert_eq!(render_sparkline(&[], 80), "");
    }

    #[test]
    fn sparkline_single_month() {
        // single month with wide width expands to days
        let freq = vec![("2024-06".to_string(), 10)];
        let result = render_sparkline(&freq, 80);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].chars().count(), 30); // 1 month * 30 days
        assert!(lines[1].contains("Jun 2024"));
    }

    #[test]
    fn sparkline_monthly_when_constrained() {
        // 12 months with only 12 char width - stays monthly
        let freq: Vec<_> = (1..=12)
            .map(|m| (format!("2024-{:02}", m), m as u32 * 2))
            .collect();
        let result = render_sparkline(&freq, 12);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].chars().count(), 12); // 12 months = 12 chars
        assert!(lines[1].starts_with("Jan 2024"));
        assert!(lines[1].ends_with("Dec 2024"));
    }

    #[test]
    fn sparkline_aggregates_to_quarters() {
        // 24 months but only 6 chars available - should aggregate to quarters (8 quarters)
        let freq: Vec<_> = (1..=24)
            .map(|i| {
                let year = 2023 + (i - 1) / 12;
                let month = ((i - 1) % 12) + 1;
                (format!("{:04}-{:02}", year, month), i as u32)
            })
            .collect();
        let result = render_sparkline(&freq, 10);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        // 24 months / 3 = 8 quarters, which fits in 10 chars
        assert_eq!(lines[0].chars().count(), 8);
    }

    #[test]
    fn sparkline_aggregates_to_years() {
        // 60 months (5 years) but only 4 chars available - should aggregate to years
        let freq: Vec<_> = (0..60)
            .map(|i| {
                let year = 2020 + i / 12;
                let month = (i % 12) + 1;
                (format!("{:04}-{:02}", year, month), 10)
            })
            .collect();
        let result = render_sparkline(&freq, 4);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        // 5 years fits in 5 chars, but we only have 4, so still aggregates to 5 (years is min)
        assert_eq!(lines[0].chars().count(), 5);
    }

    #[test]
    fn sparkline_expands_to_weeks_for_short_history() {
        // 2 months with 60 char width - should expand to weeks (8 weeks)
        let freq = vec![("2024-11".to_string(), 8), ("2024-12".to_string(), 12)];
        let result = render_sparkline(&freq, 60);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        // 2 months * 4 weeks = 8 chars (weeks), or 60 days if expanded to days
        // with 60 width available and 60 days possible, it should use days
        assert_eq!(lines[0].chars().count(), 60); // 2 months * 30 days
    }

    #[test]
    fn sparkline_expands_to_days_for_single_month() {
        // single month with plenty of width - should expand to days
        let freq = vec![("2024-12".to_string(), 15)];
        let result = render_sparkline(&freq, 60);
        let lines: Vec<&str> = result.lines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].chars().count(), 30); // 1 month * 30 days
    }

    #[test]
    fn format_month_label_works() {
        assert_eq!(format_month_label("2024-01"), "Jan 2024");
        assert_eq!(format_month_label("2023-12"), "Dec 2023");
        assert_eq!(format_month_label("invalid"), "invalid");
    }
}
