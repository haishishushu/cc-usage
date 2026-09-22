#[derive(Debug, PartialEq, Eq)]
pub struct Plan { pub show_main: bool }

pub fn plan(automatic: bool, silent: bool) -> Plan {
    Plan { show_main: automatic && !silent }
}

pub fn command(executable: &std::path::Path) -> String {
    format!("\"{}\" --autostart", executable.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_choice_applies_only_to_automatic_launches() {
        assert_eq!(plan(false, false), Plan { show_main: false });
        assert_eq!(plan(false, true), Plan { show_main: false });
        assert_eq!(plan(true, false), Plan { show_main: true });
        assert_eq!(plan(true, true), Plan { show_main: false });
    }

    #[test]
    fn autostart_command_quotes_executable_and_marks_launch_source() {
        assert_eq!(command(std::path::Path::new(r"C:\Program Files\CC Usage\app.exe")), r#""C:\Program Files\CC Usage\app.exe" --autostart"#);
    }
}
