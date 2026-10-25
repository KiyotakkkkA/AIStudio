pub mod bundled;
pub mod library;
pub mod manifest;
pub mod matching;
pub mod nodes;
pub mod script;
pub mod service;

#[cfg(test)]
mod tests;

pub use bundled::{BUNDLED_SKILLS, BundledFile, BundledSkill};
pub use library::{AuthoredSkill, SKILLS_DIR, SkillLibrary, SkillLibraryOptions};
pub use nodes::{SKILL_GENERATE, SKILL_LOAD, SKILL_SAVE, SKILL_TOOLS, register_skill_nodes};
pub use service::{
    SkillCatalogue, SkillCatalogueEntry, SkillService, SkillServiceOptions, catalogue_text,
};
