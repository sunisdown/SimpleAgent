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
        if let Some(body) = stripped.strip_prefix('!') {
            return RouteResult {
                kind: RouteKind::Command,
                command: Some("shell".to_string()),
                args: body.trim().to_string(),
            };
        }

        if !stripped.starts_with('/') {
            return RouteResult {
                kind: RouteKind::Natural,
                command: None,
                args: String::new(),
            };
        }

        let body = stripped.trim_start_matches('/').trim();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_slash_command() {
        let route = Router.route("/tools");
        assert_eq!(route.kind, RouteKind::Command);
        assert_eq!(route.command.as_deref(), Some("tools"));
        assert_eq!(route.args, "");
    }

    #[test]
    fn routes_bang_shell_command() {
        let route = Router.route("!echo hi");
        assert_eq!(route.kind, RouteKind::Command);
        assert_eq!(route.command.as_deref(), Some("shell"));
        assert_eq!(route.args, "echo hi");
    }

    #[test]
    fn routes_natural_text() {
        let route = Router.route("hello agent");
        assert_eq!(route.kind, RouteKind::Natural);
        assert_eq!(route.command, None);
    }
}
