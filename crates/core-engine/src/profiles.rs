use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppRule {
    pub application: String,
    pub profile: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProfileSettings {
    pub active_profile: String,
    pub rules: Vec<AppRule>,
}
impl Default for ProfileSettings {
    fn default() -> Self {
        Self {
            active_profile: "work".into(),
            rules: Vec::new(),
        }
    }
}
impl ProfileSettings {
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.active_profile.as_str(), "work" | "personal") || self.rules.len() > 100 {
            return Err("업무 또는 개인 프로필을 선택해 주세요".into());
        }
        let mut apps = std::collections::HashSet::new();
        for rule in &self.rules {
            if !matches!(rule.profile.as_str(), "work" | "personal")
                || !rule.application.to_ascii_lowercase().ends_with(".exe")
                || rule.application.len() > 128
                || rule.application.contains(['/', '\\', '*', '?', ':'])
                || !apps.insert(rule.application.to_ascii_lowercase())
            {
                return Err("중복 없이 앱 실행 파일 이름과 프로필을 지정해 주세요".into());
            }
        }
        Ok(())
    }
    pub fn effective(&self, app: &str) -> &str {
        self.rules
            .iter()
            .find(|r| r.application.eq_ignore_ascii_case(app))
            .map(|r| r.profile.as_str())
            .unwrap_or(&self.active_profile)
    }
}
#[derive(Default)]
pub struct ProfileContext {
    active: AtomicU8,
    pid: AtomicU32,
}
impl ProfileContext {
    pub fn set(&self, profile: &str) {
        self.active.store(
            match profile {
                "work" => 1,
                "personal" => 2,
                _ => 0,
            },
            Ordering::Release,
        );
    }
    pub fn get(&self) -> &'static str {
        match self.active.load(Ordering::Acquire) {
            1 => "work",
            2 => "personal",
            _ => "common",
        }
    }
    pub fn set_target(&self, pid: u32, profile: &str) {
        self.pid.store(0, Ordering::Release);
        self.set(profile);
        self.pid.store(pid, Ordering::Release);
    }
    pub fn for_process(&self, pid: u32) -> &'static str {
        if self.pid.load(Ordering::Acquire) != pid {
            return "common";
        }
        let profile = self.get();
        if self.pid.load(Ordering::Acquire) == pid {
            profile
        } else {
            "common"
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rules_override_default_and_unknown_processes_get_common_only() {
        let settings = ProfileSettings {
            active_profile: "work".into(),
            rules: vec![AppRule {
                application: "mail.exe".into(),
                profile: "personal".into(),
            }],
        };
        assert_eq!(settings.effective("MAIL.EXE"), "personal");
        assert_eq!(settings.effective("editor.exe"), "work");
        assert!(settings.validate().is_ok());
        let mut invalid = settings.clone();
        invalid.rules.push(invalid.rules[0].clone());
        assert!(invalid.validate().is_err());
        let context = ProfileContext::default();
        context.set_target(10, "work");
        assert_eq!(context.for_process(10), "work");
        assert_eq!(context.for_process(11), "common");
    }
}
