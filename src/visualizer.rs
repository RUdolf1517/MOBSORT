use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::algorithm::Item;

pub fn draw_lane(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[Option<Item>],
    color: Color,
    highlight: Option<usize>,
    min_value: i64,
    max_value: i64,
    scroll: usize,
) {
    let block = Block::default().title(title).borders(Borders::ALL);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }
    if items.is_empty() {
        frame.render_widget(Paragraph::new("  —"), inner);
        return;
    }

    let label_width = items
        .iter()
        .flatten()
        .map(|item| item.value.to_string().len())
        .max()
        .unwrap_or(1)
        .max(2);
    let desired_column_width = label_width.saturating_add(1);
    let column_width = desired_column_width.min(inner.width as usize).max(1);
    let visible_count = ((inner.width as usize) / column_width).max(1);
    let start = scroll.min(items.len().saturating_sub(1));
    let end = start.saturating_add(visible_count).min(items.len());
    let visible = &items[start..end];

    let graph_height = inner.height.saturating_sub(1) as usize;
    if graph_height == 0 {
        return;
    }
    let low = min_value.min(0) as f64;
    let high = max_value.max(0) as f64;
    let range = high - low;
    let zero_row = if range == 0.0 || graph_height <= 1 {
        graph_height.saturating_sub(1)
    } else {
        ((high / range) * (graph_height.saturating_sub(1) as f64)).round() as usize
    }
    .min(graph_height.saturating_sub(1));

    let mut lines = Vec::with_capacity(inner.height as usize);
    for row in 0..graph_height {
        let mut spans = Vec::with_capacity(visible.len());
        for item in visible {
            let Some(item) = item else {
                spans.push(Span::raw(" ".repeat(column_width)));
                continue;
            };

            let bar_rows = rows_for_value(item.value, low, high, zero_row, graph_height);
            let symbol = if bar_rows.contains(&row) {
                "█"
            } else if row == zero_row {
                "─"
            } else {
                " "
            };
            let style = if highlight == Some(item.id) {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            let mut cell = String::with_capacity(column_width);
            cell.push_str(symbol);
            cell.push_str(&" ".repeat(column_width.saturating_sub(1)));
            spans.push(Span::styled(cell, style));
        }
        lines.push(Line::from(spans));
    }

    let mut labels = Vec::with_capacity(visible.len());
    for item in visible {
        let Some(item) = item else {
            labels.push(Span::raw(" ".repeat(column_width)));
            continue;
        };
        let label = item.value.to_string();
        let label = truncate_to_width(&label, column_width);
        let padding = column_width.saturating_sub(label.chars().count());
        let style = if highlight == Some(item.id) {
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };
        labels.push(Span::styled(format!("{label}{}", " ".repeat(padding)), style));
    }
    lines.push(Line::from(labels));

    frame.render_widget(Paragraph::new(lines), inner);
}

fn rows_for_value(
    value: i64,
    low: f64,
    high: f64,
    zero_row: usize,
    graph_height: usize,
) -> std::ops::RangeInclusive<usize> {
    let range = high - low;
    if range == 0.0 || graph_height <= 1 || value == 0 {
        return zero_row..=zero_row;
    }

    let cells = ((value.unsigned_abs() as f64 / range) * (graph_height - 1) as f64)
        .round() as usize;
    if value > 0 {
        let top = zero_row.saturating_sub(cells);
        top..=zero_row.saturating_sub(1).max(top)
    } else {
        let bottom = zero_row.saturating_add(cells).min(graph_height - 1);
        zero_row.saturating_add(1).min(bottom)..=bottom
    }
}

fn truncate_to_width(value: &str, width: usize) -> String {
    value.chars().take(width).collect()
}
