pub(crate) const MAX_PAGEFILE_MB: u64 = 1_048_576;
#[cfg(test)]
pub(crate) const GIB_MB: u64 = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PagefileSetting {
    pub(crate) path: String,
    pub(crate) initial_size: u32,
    pub(crate) maximum_size: u32,
    pub(crate) object_path: String,
    pub(crate) relative_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PagefileInventory {
    pub(crate) automatic_managed: bool,
    pub(crate) computer_object_path: String,
    pub(crate) computer_relative_path: String,
    pub(crate) system_drive: String,
    pub(crate) physical_ram_mb: u64,
    pub(crate) free_space_mb: u64,
    pub(crate) settings: Vec<PagefileSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PagefileBinding {
    pub(crate) step: String,
    pub(crate) target_path: String,
    pub(crate) initial_size: u32,
    pub(crate) maximum_size: u32,
    pub(crate) before: PagefileInventory,
    pub(crate) target: Option<PagefileSetting>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedPagefileToken {
    pub(crate) object_path: String,
    pub(crate) relative_path: String,
    pub(crate) path: String,
    pub(crate) initial_size: u32,
    pub(crate) maximum_size: u32,
}

#[cfg(any(test, windows))]
pub(crate) trait PagefileStore {
    fn inventory(&mut self) -> Result<PagefileInventory, String>;
    fn set_automatic(
        &mut self,
        object_path: &str,
        relative_path: &str,
        expected: Option<bool>,
        value: bool,
    ) -> Result<(), String>;
    fn update(
        &mut self,
        setting: &PagefileSetting,
        initial: u32,
        maximum: u32,
    ) -> Result<(), String>;
    fn create(
        &mut self,
        path: &str,
        initial: u32,
        maximum: u32,
    ) -> Result<CreatedPagefileToken, String>;
    fn delete(&mut self, created: &CreatedPagefileToken) -> Result<(), String>;
}
