use crate::*;
mod system_tool_tests {
    use std::path::Path;

    use super::{CommandName, resolve_system_tool_path};

    #[test]
    pub(crate) fn system_tools_resolve_beneath_the_trusted_directory() {
        let system_directory = Path::new("/trusted/System32");
        for command in [
            CommandName::Bcdedit,
            CommandName::Powercfg,
            CommandName::Pnputil,
            CommandName::Fsutil,
            CommandName::Defrag,
        ] {
            assert_eq!(
                resolve_system_tool_path(system_directory, command.program()),
                Ok(system_directory.join(command.program()))
            );
        }
    }

    #[test]
    pub(crate) fn system_tool_resolution_rejects_unexpected_program_names() {
        let system_directory = Path::new("/trusted/System32");
        for program in ["cmd.exe", "bcdedit", "bcdedit.exe.bak", r"..\bcdedit.exe"] {
            assert_eq!(
                resolve_system_tool_path(system_directory, program),
                Err("system tool is not allowlisted".into())
            );
        }
    }
}
