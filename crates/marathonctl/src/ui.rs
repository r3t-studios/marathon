//! Beautiful CLI UI module for marathonctl using ratatui
//!
//! Provides simple builder APIs for rendering beautiful terminal output
//! without taking over the terminal (inline mode).
//!
//! # Examples
//!
//! ```rust
//! // Render a status table
//! ui::table("Session Status")
//!     .row("Node ID", node_id)
//!     .row("Session", session_id)
//!     .row("Queue Size", queue_size)
//!     .render();
//!
//! // Render a list
//! ui::list("Connected Peers").item(peer1).item(peer2).render();
//!
//! // Render a data grid
//! ui::grid("Sessions")
//!     .header(&["ID", "State", "Entities"])
//!     .row(&[id1, state1, count1])
//!     .row(&[id2, state2, count2])
//!     .render();
//! ```

use ratatui::{
    TerminalOptions,
    Viewport,
    prelude::*,
    widgets::*,
};

/// Create a key-value table (like status output)
pub fn table(title: &str) -> TableBuilder {
    TableBuilder::new(title)
}

/// Create a simple list
pub fn list(title: &str) -> ListBuilder {
    ListBuilder::new(title)
}

/// Create a data grid with headers
pub fn grid(title: &str) -> GridBuilder {
    GridBuilder::new(title)
}

/// Builder for key-value tables
pub struct TableBuilder {
    title: String,
    rows: Vec<(String, String)>,
}

impl TableBuilder {
    fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            rows: Vec::new(),
        }
    }

    /// Add a key-value row
    pub fn row(mut self, key: impl std::fmt::Display, value: impl std::fmt::Display) -> Self {
        self.rows.push((key.to_string(), value.to_string()));
        self
    }

    /// Render the table to the terminal
    pub fn render(self) {
        let height = self.rows.len() + 4; // rows + borders + header + padding
        render_widget(height, |frame| {
            let area = frame.area();

            let rows: Vec<Row> = self
                .rows
                .iter()
                .map(|(k, v)| Row::new(vec![k.clone(), v.clone()]))
                .collect();

            let table = Table::new(rows, [Constraint::Length(20), Constraint::Min(30)])
                .header(
                    Row::new(vec!["Field", "Value"])
                        .bold()
                        .style(Style::default().fg(Color::Green)),
                )
                .block(
                    Block::default()
                        .title(format!(" {} ", self.title))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .column_spacing(2);

            frame.render_widget(table, area);
        });
    }
}

/// Builder for simple lists
pub struct ListBuilder {
    title: String,
    items: Vec<String>,
}

impl ListBuilder {
    fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            items: Vec::new(),
        }
    }

    /// Add an item to the list
    pub fn item(mut self, item: impl std::fmt::Display) -> Self {
        self.items.push(item.to_string());
        self
    }

    /// Render the list to the terminal
    pub fn render(self) {
        let height = self.items.len() + 3; // items + borders + title
        render_widget(height, |frame| {
            let area = frame.area();

            let items: Vec<ListItem> = self
                .items
                .iter()
                .map(|i| ListItem::new(format!("  • {}", i)))
                .collect();

            let list = List::new(items).block(
                Block::default()
                    .title(format!(" {} ", self.title))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Cyan)),
            );

            frame.render_widget(list, area);
        });
    }
}

/// Builder for data grids with headers
pub struct GridBuilder {
    title: String,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl GridBuilder {
    fn new(title: &str) -> Self {
        Self {
            title: title.to_string(),
            headers: Vec::new(),
            rows: Vec::new(),
        }
    }

    /// Set the header row
    pub fn header(mut self, headers: &[impl std::fmt::Display]) -> Self {
        self.headers = headers.iter().map(|h| h.to_string()).collect();
        self
    }

    /// Add a data row
    pub fn row(mut self, cells: &[impl std::fmt::Display]) -> Self {
        self.rows
            .push(cells.iter().map(|c| c.to_string()).collect());
        self
    }

    /// Render the grid to the terminal
    pub fn render(self) {
        let height = self.rows.len() + 4; // rows + borders + header + padding
        render_widget(height, |frame| {
            let area = frame.area();

            // Create constraints based on number of columns
            let col_count = self
                .headers
                .len()
                .max(self.rows.iter().map(|r| r.len()).max().unwrap_or(0));
            let constraints = vec![Constraint::Ratio(1, col_count as u32); col_count];

            let header = Row::new(self.headers.clone())
                .bold()
                .style(Style::default().fg(Color::Green));

            let rows: Vec<Row> = self.rows.iter().map(|r| Row::new(r.clone())).collect();

            let table = Table::new(rows, constraints)
                .header(header)
                .block(
                    Block::default()
                        .title(format!(" {} ", self.title))
                        .borders(Borders::ALL)
                        .border_style(Style::default().fg(Color::Cyan)),
                )
                .column_spacing(2);

            frame.render_widget(table, area);
        });
    }
}

/// Internal helper to render a widget using inline viewport
fn render_widget<F>(height: usize, render_fn: F)
where
    F: FnOnce(&mut Frame), {
    let mut terminal = ratatui::init_with_options(TerminalOptions {
        viewport: Viewport::Inline(height as u16),
    });

    terminal.draw(render_fn).unwrap();
    ratatui::restore();
}
