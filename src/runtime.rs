use crate::tools::{create_profile_tools, ToolRegistry};

#[derive(Clone, Debug)]
pub enum RuntimeProfile {
    Yolo,
    Readonly,
    Custom(Vec<String>),
}

impl RuntimeProfile {
    pub fn parse(name: &str, custom_tools: Option<&str>) -> Result<Self, String> {
        match name {
            "yolo" => Ok(Self::Yolo),
            "readonly" => Ok(Self::Readonly),
            "custom" => {
                let raw = custom_tools
                    .ok_or_else(|| "--tools is required for custom profile".to_string())?;
                let parsed = raw
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                if parsed.is_empty() {
                    return Err("custom profile needs at least one tool".to_string());
                }
                Ok(Self::Custom(parsed))
            }
            _ => Err("profile must be one of: yolo, readonly, custom".to_string()),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            RuntimeProfile::Yolo => "yolo",
            RuntimeProfile::Readonly => "readonly",
            RuntimeProfile::Custom(_) => "custom",
        }
    }

    pub fn shell_route_allowed(&self) -> bool {
        matches!(self, RuntimeProfile::Yolo | RuntimeProfile::Custom(_))
    }

    pub fn tool_registry(&self) -> Result<ToolRegistry, String> {
        Ok(ToolRegistry::new(create_profile_tools(self)?))
    }
}
