use std::ops::Range;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::algorithm::Item;

#[derive(Clone, Copy, Debug, Default)]
struct DisplayColumn {
    min_value: Option<i64>,
    max_value: Option<i64>,
    label_value: Option<i64>,
    highlighted: bool,
}

pub fn draw_lane(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    items: &[Option<Item>],
    color: Color,
    highlight: Option<usize>,
    min_value: i64,
    max_value: i64,
    fit_all: bool,
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
    let (columns, column_width, scale_caption): (Vec<DisplayColumn>, usize, Option<String>) =
        if fit_all {
            let column_count = fit_column_count(items.len(), inner.width as usize);
            let available_width = inner.width as usize / column_count;
            let column_width = desired_column_width.min(available_width).max(1);
            let labels_fit = column_width >= desired_column_width;
            let columns = (0..column_count)
                .map(|index| {
                    let range = bucket_bounds(index, items.len(), column_count);
                    summarize_column(&items[range], highlight, labels_fit)
                })
                .collect();
            let elements_per_column = items.len().saturating_add(column_count - 1) / column_count;
            let caption = (!labels_fit).then(|| scale_caption(elements_per_column));
            (columns, column_width, caption)
        } else {
            let column_width = desired_column_width.min(inner.width as usize).max(1);
            let visible_count = ((inner.width as usize) / column_width).max(1);
            let start = scroll.min(items.len().saturating_sub(1));
            let end = start.saturating_add(visible_count).min(items.len());
            let columns = (start..end)
                .map(|index| summarize_column(&items[index..=index], highlight, true))
                .collect();
            (columns, column_width, None)
        };

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
        let mut spans = Vec::with_capacity(columns.len());
        for column in &columns {
            let symbol = match (column.min_value, column.max_value) {
                (Some(minimum), Some(maximum)) => {
                    let minimum_rows = rows_for_value(minimum, low, high, zero_row, graph_height);
                    let maximum_rows = rows_for_value(maximum, low, high, zero_row, graph_height);
                    let in_minimum = minimum_rows.contains(&row);
                    let in_maximum = maximum_rows.contains(&row);
                    if in_minimum && in_maximum {
                        "█"
                    } else if in_minimum || in_maximum {
                        "▒"
                    } else if row == zero_row {
                        "─"
                    } else {
                        " "
                    }
                }
                _ => " ",
            };
            let style = if column.highlighted {
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
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

    if let Some(caption) = scale_caption {
        lines.push(Line::from(Span::styled(
            caption,
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        let labels = columns
            .iter()
            .map(|column| {
                let Some(value) = column.label_value else {
                    return Span::raw(" ".repeat(column_width));
                };
                let label = truncate_to_width(&value.to_string(), column_width);
                let padding = column_width.saturating_sub(label.chars().count());
                let style = if column.highlighted {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(color)
                };
                Span::styled(format!("{label}{}", " ".repeat(padding)), style)
            })
            .collect::<Vec<_>>();
        lines.push(Line::from(labels));
    }

    frame.render_widget(Paragraph::new(lines), inner);
}

fn fit_column_count(item_count: usize, width: usize) -> usize {
    item_count.min(width).max(1)
}

fn bucket_bounds(index: usize, item_count: usize, column_count: usize) -> Range<usize> {
    let start = index * item_count / column_count;
    let end = (index + 1) * item_count / column_count;
    start..end
}

fn summarize_column(
    items: &[Option<Item>],
    highlight: Option<usize>,
    include_label: bool,
) -> DisplayColumn {
    let mut min_value = None;
    let mut max_value = None;
    let mut item_count = 0usize;
    let mut highlighted = false;

    for item in items.iter().flatten() {
        min_value = Some(min_value.map_or(item.value, |value: i64| value.min(item.value)));
        max_value = Some(max_value.map_or(item.value, |value: i64| value.max(item.value)));
        item_count += 1;
        highlighted |= highlight == Some(item.id);
    }

    DisplayColumn {
        min_value,
        max_value,
        label_value: if include_label && item_count == 1 {
            min_value
        } else {
            None
        },
        highlighted,
    }
}

fn scale_caption(elements_per_column: usize) -> String {
    if elements_per_column == 1 {
        "1 ячейка = 1 элемент".to_string()
    } else {
        format!("≈{elements_per_column} элементов/ячейку")
    }
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

#[cfg(test)]
mod tests {
    use super::{bucket_bounds, fit_column_count, summarize_column, Item};

    #[test]
    fn fit_view_uses_one_column_per_element_when_it_fits() {
        let count = fit_column_count(50, 80);
        assert_eq!(count, 50);
        for index in 0..count {
            assert_eq!(bucket_bounds(index, 50, count), index..index + 1);
        }
    }

    #[test]
    fn fit_view_distributes_every_element_across_available_columns() {
        let item_count = 3_000;
        let column_count = fit_column_count(item_count, 80);
        assert_eq!(column_count, 80);

        let mut next_item = 0;
        for index in 0..column_count {
            let bucket = bucket_bounds(index, item_count, column_count);
            assert_eq!(bucket.start, next_item);
            assert!(bucket.end > bucket.start);
            next_item = bucket.end;
        }
        assert_eq!(next_item, item_count);
    }

    #[test]
    fn compact_column_preserves_value_range_and_movement_highlight() {
        let column = summarize_column(
            &[
                Some(Item { id: 0, value: 7 }),
                Some(Item { id: 1, value: -4 }),
                Some(Item { id: 2, value: 12 }),
            ],
            Some(1),
            false,
        );
        assert_eq!(column.min_value, Some(-4));
        assert_eq!(column.max_value, Some(12));
        assert_eq!(column.label_value, None);
        assert!(column.highlighted);
    }
}
