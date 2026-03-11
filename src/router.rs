#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteKind {
    Command,
    Shell,
    Prompt,
}

#[derive(Clone, Debug)]
pub struct Route {
    pub kind: RouteKind,
    pub command: Option<String>,
    pub args: String,
}

pub struct Router;

impl Router {
    pub fn route(&self, input: &str) -> Route {
        let trimmed = input.trim();
        if let Some(rest) = trimmed.strip_prefix('!') {
            return Route {
                kind: RouteKind::Shell,
                command: Some("shell".to_string()),
                args: rest.trim().to_string(),
            };
        }

        if let Some(rest) = trimmed.strip_prefix('/') {
            let mut parts = rest.splitn(2, ' ');
            return Route {
                kind: RouteKind::Command,
                command: parts.next().map(|s| s.trim().to_string()),
                args: parts.next().unwrap_or_default().trim().to_string(),
            };
        }

        Route {
            kind: RouteKind::Prompt,
            command: None,
            args: trimmed.to_string(),
        }
    }
}
