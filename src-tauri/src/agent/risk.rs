#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolRisk {
    ReadOnly,
    Reversible,
    Modifying,
    Destructive,
    External,
}

pub struct RiskClassifier;

impl RiskClassifier {
    pub fn new() -> Self {
        Self
    }

    pub fn classify(&self, tool_name: &str) -> ToolRisk {
        match tool_name {
            "search.rg" => ToolRisk::ReadOnly,
            "files.read" | "files.open" => ToolRisk::ReadOnly,
            "files.append" => ToolRisk::Modifying,
            "files.create" => ToolRisk::Modifying,
            "files.write" | "files.replace" | "files.edit" => ToolRisk::Modifying,
            name if name.starts_with("http.") || name.starts_with("net.") => ToolRisk::External,
            name if name.contains("delete") || name.contains("remove") => ToolRisk::Destructive,
            _ => ToolRisk::Modifying,
        }
    }

    pub fn requires_approval(&self, risk: ToolRisk) -> bool {
        match risk {
            ToolRisk::ReadOnly | ToolRisk::Reversible => false,
            ToolRisk::Modifying | ToolRisk::Destructive | ToolRisk::External => true,
        }
    }
}
