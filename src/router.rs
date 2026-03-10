#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteKind {
    Command,
    Natural,
}

#[derive(Debug, Clone)]
pub struct RouteResult {
    pub kind: RouteKind,
    pub command: Option<String>,
    pub args: String,
}

pub struct Router;

impl Router {
    pub fn route(&self, text: &str) -> RouteResult {
        let stripped = text.trim();
        if !stripped.starts_with(',') {
            return RouteResult {
                kind: RouteKind::Natural,
                command: None,
                args: String::new(),
            };
        }

        let body = stripped.trim_start_matches(',').trim();
        if body.is_empty() {
            return RouteResult {
                kind: RouteKind::Command,
                command: Some("help".to_string()),
                args: String::new(),
            };
        }

        let mut parts = body.splitn(2, ' ');
        let command = parts.next().unwrap_or("help").to_lowercase();
        let args = parts.next().unwrap_or("").trim().to_string();
        RouteResult {
            kind: RouteKind::Command,
            command: Some(command),
            args,
        }
    }
}
