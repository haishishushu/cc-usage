//! 隔离验收只在调试构建中启用，正式程序始终使用正常数据目录。
pub fn data_dir() -> Option<std::path::PathBuf> {
    #[cfg(debug_assertions)]
    if let Some(value) = std::env::var_os("CC_USAGE_TEST_DIR") {
        let path = std::path::PathBuf::from(value);
        assert!(path.is_absolute() && path.join(".acceptance-profile").is_file(),
            "验收目录必须是绝对路径且包含 .acceptance-profile 标记");
        return Some(path);
    }
    None
}

pub fn isolated() -> bool { data_dir().is_some() }
