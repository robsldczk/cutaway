//! Category and tags from a file name (AI `sound_tag` refines unclear cases).

const RULES: &[(&str, &str)] = &[
    ("door", "SFX/Doors"),
    ("knock", "SFX/Doors"),
    ("alarm", "SFX/Alarms"),
    ("siren", "SFX/Alarms"),
    ("beep", "SFX/Machines"),
    ("hum", "SFX/Machines"),
    ("machine", "SFX/Machines"),
    ("computer", "SFX/Machines"),
    ("phone", "SFX/Phones"),
    ("ring", "SFX/Phones"),
    ("step", "SFX/Foley"),
    ("foley", "SFX/Foley"),
    ("cloth", "SFX/Foley"),
    ("paper", "SFX/Foley"),
    ("hit", "SFX/Impacts"),
    ("impact", "SFX/Impacts"),
    ("crash", "SFX/Impacts"),
    ("car", "SFX/Vehicles"),
    ("engine", "SFX/Vehicles"),
    ("bird", "SFX/Animals"),
    ("turkey", "SFX/Animals"),
    ("dog", "SFX/Animals"),
    ("roomtone", "Ambience"),
    ("room tone", "Ambience"),
    ("ambience", "Ambience"),
    ("amb", "Ambience"),
    ("atmo", "Ambience"),
    ("music", "Music"),
    ("theme", "Music"),
    ("score", "Music"),
];

/// Returns `(category, tags)`. `None` category means unclear (ask the AI).
pub fn tag_from_name(file_name: &str) -> (Option<String>, Vec<String>) {
    let stem = file_name
        .rsplit_once('.')
        .map(|(s, _)| s)
        .unwrap_or(file_name);
    let low = stem.to_lowercase().replace(['_', '-'], " ");
    let cat = RULES
        .iter()
        .find(|(k, _)| low.contains(k))
        .map(|(_, c)| c.to_string());
    let tags = low
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| t.len() > 2 && !t.chars().all(|c| c.is_ascii_digit()))
        .map(String::from)
        .collect();
    (cat, tags)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(
            tag_from_name("Door_Slam_02.wav").0.as_deref(),
            Some("SFX/Doors")
        );
        assert_eq!(
            tag_from_name("canteen-roomtone.wav").0.as_deref(),
            Some("Ambience")
        );
        assert_eq!(tag_from_name("x123.wav").0, None);
        assert_eq!(tag_from_name("Door_Slam_02.wav").1, vec!["door", "slam"]);
    }
}
