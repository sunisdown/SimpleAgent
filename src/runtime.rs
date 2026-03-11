use crate::tools::{create_profile_tools, ToolRegistry};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeProfile {
    Yolo,
    Readonly,
    Custom(Vec<String>),
}

impl RuntimeProfile {
    pub fn parse(name: &str, custom_tools: Option<&str>) -> Result<Self, String> {
        match name.trim().to_lowercase().as_str() {
            "yolo" => Ok(Self::Yolo),
            "readonly" => Ok(Self::Readonly),
            "custom" => {
                let tools = custom_tools
                    .unwrap_or_default()
                    .split(',')
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>();
                if tools.is_empty() {
                    return Err(
                        "--profile custom requires --tools <comma-separated tool names>"
                            .to_string(),
                    );
                }
                Ok(Self::Custom(tools))
            }
            other => Err(format!(
                "unknown profile '{other}'. Expected one of: yolo, readonly, custom"
            )),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Yolo => "yolo",
            Self::Readonly => "readonly",
            Self::Custom(_) => "custom",
        }
    }

    pub fn shell_route_allowed(&self) -> bool {
        !matches!(self, Self::Readonly)
    }

    pub fn tool_registry(&self) -> Result<ToolRegistry, String> {
        Ok(ToolRegistry::new(create_profile_tools(self)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_custom_requires_tools() {
        let err = RuntimeProfile::parse("custom", None).expect_err("should fail");
        assert!(err.contains("requires --tools"));
    }

    #[test]
    fn readonly_disables_shell_route() {
        assert!(!RuntimeProfile::Readonly.shell_route_allowed());
    }
}
