use super::CompiledScript;
use crate::{Script, ValidationError};
use serde::{Deserialize, Serialize};

/// Structural YAML paths use one-based list indices. No prepared values are kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallSite {
    pub sequence: String,
    pub location: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceOrigin {
    pub location: String,
    pub section: Option<String>,
    pub phase: String,
    pub call_chain: Vec<CallSite>,
    pub generated: Option<String>,
}
impl SourceOrigin {
    pub(super) fn plain(location: String, section: Option<&str>, phase: &str) -> Self {
        Self {
            location,
            section: section.map(str::to_owned),
            phase: phase.into(),
            call_chain: vec![],
            generated: None,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct SourceMap {
    pub steps: Vec<SourceOrigin>,
    pub sections: Vec<SectionMap>,
}
#[derive(Debug, Default)]
pub(super) struct SectionMap {
    pub setup: Vec<SourceOrigin>,
    pub reset: Vec<SourceOrigin>,
    pub requires: Vec<SourceOrigin>,
    pub steps: Vec<SourceOrigin>,
}
pub(super) fn origins(
    at: &str,
    count: usize,
    section: Option<&str>,
    phase: &str,
) -> Vec<SourceOrigin> {
    (1..=count)
        .map(|index| {
            let mut source = SourceOrigin::plain(format!("{at}[{index}]"), section, phase);
            if phase == "requires" {
                source.generated = Some("requires".into());
            }
            source
        })
        .collect()
}
impl SourceMap {
    pub fn legacy(script: &Script) -> Result<Self, ValidationError> {
        let names = script
            .sections
            .iter()
            .map(|section| {
                section.name.len().saturating_mul(
                    section.setup.len()
                        + section.reset.as_ref().map_or(0, Vec::len)
                        + section.requires.len()
                        + section.steps.len(),
                )
            })
            .fold(0usize, usize::saturating_add);
        if names > crate::MAX_SCRIPT_BYTES {
            return Err(ValidationError::at(
                "source_map",
                "source section names exceed the 4 MiB compilation budget",
            ));
        }
        Ok(Self {
            steps: origins("steps", script.steps.len(), None, "steps"),
            sections: script
                .sections
                .iter()
                .enumerate()
                .map(|(index, section)| {
                    let at = format!("sections[{}]", index + 1);
                    let name = Some(section.name.as_str());
                    SectionMap {
                        setup: origins(&format!("{at}.setup"), section.setup.len(), name, "setup"),
                        reset: origins(
                            &format!("{at}.reset"),
                            section.reset.as_ref().map_or(0, Vec::len),
                            name,
                            "reset",
                        ),
                        requires: origins(
                            &format!("{at}.requires"),
                            section.requires.len(),
                            name,
                            "requires",
                        ),
                        steps: origins(&format!("{at}.steps"), section.steps.len(), name, "steps"),
                    }
                })
                .collect(),
        })
    }
}

#[derive(Debug)]
pub struct PreparedScript {
    pub script: Script,
    pub sources: Vec<SourceOrigin>,
}
impl CompiledScript {
    /// Prepare a take with source references. Use before modifying the compiled
    /// script: locations describe the source originally passed to `compile`.
    pub fn prepare(
        &self,
        section_name: Option<&str>,
        retake: bool,
    ) -> Result<PreparedScript, ValidationError> {
        let script = self.script.prepare(section_name, retake)?;
        let mut sources = Vec::new();
        if self.script.sections.is_empty() {
            sources.extend(self.source_map.steps.clone());
        } else {
            if self.source_map.sections.len() != self.script.sections.len() {
                return Err(ValidationError::at(
                    "source_map",
                    "compiled section layout changed",
                ));
            }
            for (section, map) in self.script.sections.iter().zip(&self.source_map.sections) {
                if section_name.is_some_and(|name| name != section.name) {
                    continue;
                }
                if retake {
                    sources.extend(map.reset.clone());
                }
                sources.extend(map.setup.clone());
                sources.extend(map.requires.clone());
                sources.extend(map.steps.clone());
            }
        }
        if sources.len() != script.steps.len() {
            return Err(ValidationError::at(
                "source_map",
                "compiled action layout changed",
            ));
        }
        Ok(PreparedScript { script, sources })
    }
}
