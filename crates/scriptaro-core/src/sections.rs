use crate::{Action, Script, ValidationError};
use std::collections::HashSet;

impl Script {
    pub(crate) fn validate_sections(&self) -> Result<(), ValidationError> {
        if !self.steps.is_empty() {
            return Err(ValidationError::at(
                "sections",
                "use either steps or sections, not both",
            ));
        }
        let mut names = HashSet::new();
        let mut all = Vec::new();
        for section in &self.sections {
            if section.name.trim().is_empty()
                || section.name.chars().any(char::is_control)
                || !names.insert(&section.name)
            {
                return Err(ValidationError::at(
                    "sections",
                    "section names must be unique, nonempty, and contain no control characters",
                ));
            }
            if section.steps.is_empty() {
                return Err(ValidationError::at(
                    &section.name,
                    "section must contain steps",
                ));
            }
            all.extend(section.setup.clone());
            all.extend(section.reset.iter().flatten().cloned());
            all.extend(
                section
                    .requires
                    .iter()
                    .cloned()
                    .map(|condition| Action::WaitUntil {
                        condition,
                        timeout_ms: None,
                    }),
            );
            all.extend(section.steps.clone());
        }
        // Validate the aggregate, including every reset, before any side effect.
        let flat = Script {
            steps: all,
            sections: Vec::new(),
            ..self.clone()
        };
        flat.validate()
    }

    /// Compile a full run, single section, or explicit retake into ordinary actions.
    /// A retake requires a named section and an explicitly authored reset (an empty
    /// reset is allowed when the author declares that external restoration suffices).
    /// Every run uses a fresh engine; this never resumes an interrupted side effect.
    pub fn prepare(
        &self,
        section_name: Option<&str>,
        retake: bool,
    ) -> Result<Script, ValidationError> {
        self.validate()?;
        if retake && section_name.is_none() {
            return Err(ValidationError::at(
                "retake",
                "select a named section to retake",
            ));
        }
        if section_name.is_none() && self.sections.is_empty() {
            return Ok(self.clone());
        }
        let selected: Vec<_> = self
            .sections
            .iter()
            .filter(|section| section_name.is_none_or(|name| name == section.name))
            .collect();
        if selected.is_empty() {
            return Err(ValidationError::at("section", "named section not found"));
        }
        let mut steps = Vec::new();
        for section in selected {
            if retake {
                let reset = section.reset.as_ref().ok_or_else(|| {
                    ValidationError::at(&section.name, "retake requires an explicit reset")
                })?;
                steps.extend(reset.clone());
            }
            steps.extend(section.setup.clone());
            steps.extend(
                section
                    .requires
                    .iter()
                    .cloned()
                    .map(|condition| Action::WaitUntil {
                        condition,
                        timeout_ms: None,
                    }),
            );
            steps.extend(section.steps.clone());
        }
        let prepared = Script {
            steps,
            sections: Vec::new(),
            ..self.clone()
        };
        prepared.validate()?;
        Ok(prepared)
    }
}
