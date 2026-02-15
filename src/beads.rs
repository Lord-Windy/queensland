//! Query and parse beads tickets.

use std::process::Command;
use std::str::FromStr;

/// Ticket priority levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    P0,
    P1,
    P2,
    P3,
    P4,
}

impl FromStr for Priority {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "P0" => Ok(Self::P0),
            "P1" => Ok(Self::P1),
            "P2" => Ok(Self::P2),
            "P3" => Ok(Self::P3),
            "P4" => Ok(Self::P4),
            _ => Err(()),
        }
    }
}

/// Ticket type categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketType {
    Epic,
    Task,
    Bug,
    Feature,
}

impl FromStr for TicketType {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        match s {
            "epic" => Ok(Self::Epic),
            "task" => Ok(Self::Task),
            "bug" => Ok(Self::Bug),
            "feature" => Ok(Self::Feature),
            _ => Err(()),
        }
    }
}

/// A parsed beads ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub priority: Priority,
    pub ticket_type: TicketType,
}

/// Run `bd ready` and parse the output into tickets.
pub fn ready() -> crate::error::Result<Vec<Ticket>> {
    let output = match Command::new("bd").arg("ready").output() {
        Ok(output) => output,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(crate::error::Error::BdNotFound);
        }
        Err(e) => return Err(e.into()),
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let tickets: Vec<Ticket> = stdout.lines().filter_map(parse_line).collect();

    if tickets.is_empty() {
        return Err(crate::error::Error::NoReadyTickets);
    }

    Ok(tickets)
}

/// Parse a single line of `bd ready` output into a `Ticket`.
///
/// Expected format: `N. [● P<n>] [<type>] <ID>: <title>`
fn parse_line(line: &str) -> Option<Ticket> {
    // Strip the leading number and dot: "1. [● P1] ..."
    let rest = line.split_once(". ")?.1;

    // Extract priority from first bracket group: "[● P1]"
    let rest = rest.strip_prefix('[')?;
    let (bracket_content, rest) = rest.split_once("] ")?;
    let priority_str = bracket_content.split_whitespace().last()?;
    let priority = Priority::from_str(priority_str).ok()?;

    // Extract type from second bracket group: "[task]"
    let rest = rest.strip_prefix('[')?;
    let (type_str, rest) = rest.split_once("] ")?;
    let ticket_type = TicketType::from_str(type_str).ok()?;

    // Split "QLD-6su: Parse bd ready output" on first ": "
    let (id, title) = rest.split_once(": ")?;

    Some(Ticket {
        id: id.to_string(),
        title: title.to_string(),
        description: None,
        priority,
        ticket_type,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_task() {
        let line = "2. [● P1] [task] QLD-6su: Parse bd ready output";
        let ticket = parse_line(line).unwrap();
        assert_eq!(ticket.id, "QLD-6su");
        assert_eq!(ticket.title, "Parse bd ready output");
        assert_eq!(ticket.priority, Priority::P1);
        assert_eq!(ticket.ticket_type, TicketType::Task);
        assert_eq!(ticket.description, None);
    }

    #[test]
    fn test_parse_line_epic() {
        let line = "1. [● P1] [epic] QLD-e0l: M1: Beads Integration";
        let ticket = parse_line(line).unwrap();
        assert_eq!(ticket.id, "QLD-e0l");
        assert_eq!(ticket.title, "M1: Beads Integration");
        assert_eq!(ticket.priority, Priority::P1);
        assert_eq!(ticket.ticket_type, TicketType::Epic);
    }

    #[test]
    fn test_parse_line_with_colon_in_title() {
        let line = "5. [● P2] [feature] QLD-abc: Config: load and validate";
        let ticket = parse_line(line).unwrap();
        assert_eq!(ticket.id, "QLD-abc");
        assert_eq!(ticket.title, "Config: load and validate");
    }

    #[test]
    fn test_parse_header_returns_none() {
        let line = "📋 Ready work (6 issues with no blockers):";
        assert!(parse_line(line).is_none());
    }

    #[test]
    fn test_parse_blank_returns_none() {
        assert!(parse_line("").is_none());
    }

    #[test]
    fn test_ready_no_tickets() {
        // Simulate empty output by parsing no lines
        let stdout = "📋 Ready work (0 issues with no blockers):\n";
        let tickets: Vec<Ticket> = stdout.lines().filter_map(parse_line).collect();
        assert!(tickets.is_empty());
    }

    #[test]
    fn test_parse_all_priorities() {
        for (i, expected) in [
            Priority::P0,
            Priority::P1,
            Priority::P2,
            Priority::P3,
            Priority::P4,
        ]
        .iter()
        .enumerate()
        {
            let line = format!("1. [● P{}] [task] QLD-x: Title", i);
            let ticket = parse_line(&line).unwrap();
            assert_eq!(ticket.priority, *expected);
        }
    }

    #[test]
    fn test_parse_all_types() {
        for (type_str, expected) in [
            ("epic", TicketType::Epic),
            ("task", TicketType::Task),
            ("bug", TicketType::Bug),
            ("feature", TicketType::Feature),
        ] {
            let line = format!("1. [● P2] [{}] QLD-x: Title", type_str);
            let ticket = parse_line(&line).unwrap();
            assert_eq!(ticket.ticket_type, expected);
        }
    }
}
