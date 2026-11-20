//! Active-environment lookup and `{{variable}}` resolution.

use crate::app::CrabiPie;
use crate::app::tab::TabState;
use crate::domain::environment::Environment;
use crate::domain::variables::{VAR_RE, resolve_dynamic_var};

impl CrabiPie {
    /// The environment definition backing the *currently active tab's*
    /// selection. Environment definitions are shared/global; only which one
    /// a tab has selected is per-tab.
    pub(crate) fn active_environment(&self) -> Option<&Environment> {
        let env_id = self.current_tab()?.active_environment_id?;
        self.environments.iter().find(|e| e.id == env_id)
    }

    pub(crate) fn environment_for_tab(&self, tab: &TabState) -> Option<&Environment> {
        let env_id = tab.active_environment_id?;
        self.environments.iter().find(|e| e.id == env_id)
    }

    pub(crate) fn resolve_vars(&self, input: &str) -> String {
        if !input.contains("{{") {
            return input.to_string();
        }
        let env = self.active_environment();
        VAR_RE
            .replace_all(input, |caps: &regex::Captures| {
                let name = &caps[1];
                if let Some(resolved) = resolve_dynamic_var(name) {
                    return resolved;
                }
                env.and_then(|e| e.variables.iter().find(|v| v.enabled && v.key == name))
                    .map(|v| v.value.clone())
                    .unwrap_or_else(|| caps[0].to_string())
            })
            .to_string()
    }
}
