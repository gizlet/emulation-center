//! The center's words in English and Simplified Chinese, chosen by
//! `GIZLET_LANG` (`zh-CN` or `en`), as the launcher sets it.

pub struct Words {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub not_installed: &'static str,
    pub installed: &'static str,
    pub update_to: &'static str,
    pub up_to_date: &'static str,
    pub checking: &'static str,
    pub offline: &'static str,
    pub not_ready: &'static str,
    pub missing: &'static str,
    pub waiting: &'static str,
    pub downloading: &'static str,
    pub verifying: &'static str,
    pub installing: &'static str,
    pub failed: &'static str,
    pub open: &'static str,
    pub install: &'static str,
    pub update: &'static str,
    pub refresh: &'static str,
    pub quit: &'static str,
    pub choose: &'static str,
    pub back: &'static str,
    pub games: &'static str,
    pub tf2: &'static str,
    pub no_tf2: &'static str,
    pub tf2_found: &'static str,
    pub use_tf2: &'static str,
    pub change_folder: &'static str,
    pub picker_title: &'static str,
    pub use_this: &'static str,
    pub places: &'static str,
    pub home: &'static str,
    pub media: &'static str,
    pub mounts: &'static str,
    pub up: &'static str,
    pub current: &'static str,
    pub one_file: &'static str,
    pub files: &'static str,
    pub one_folder: &'static str,
    pub folders: &'static str,
    pub empty: &'static str,
    pub one_file_here: &'static str,
    pub files_here: &'static str,
    pub no_folders: &'static str,
    pub opening: &'static str,
    pub open_note: &'static str,
    pub cannot_read_apps: &'static str,
    pub wait_install: &'static str,
}

pub static EN: Words = Words {
    title: "Emulation Center",
    subtitle: "Install an emulator, then open the games you already have.",
    not_installed: "Not installed",
    installed: "Installed {v}",
    update_to: "Update to {v}",
    up_to_date: "Up to date",
    checking: "Checking the Store…",
    offline: "Store unavailable: {e}",
    not_ready: "The Store is checking a new release",
    missing: "Not in the Store yet",
    waiting: "Waiting…",
    downloading: "Downloading {p}%",
    verifying: "Checking the download {p}%",
    installing: "Installing…",
    failed: "Failed: {e}",
    open: "Open",
    install: "Install",
    update: "Update",
    refresh: "Refresh",
    quit: "Quit",
    choose: "Choose",
    back: "Back",
    games: "Games",
    tf2: "TF2 card",
    no_tf2: "No TF2 card found. Choose the folder with your games.",
    tf2_found: "TF2 card at {p}",
    use_tf2: "Use the TF2 card",
    change_folder: "Choose a folder",
    picker_title: "Choose your games folder",
    use_this: "Use this folder",
    places: "Places",
    home: "Home",
    media: "Cards and drives",
    mounts: "Mounts",
    up: "Up one level",
    current: "Current",
    one_file: "1 file",
    files: "{n} files",
    one_folder: "1 folder",
    folders: "{n} folders",
    empty: "Empty",
    one_file_here: "1 file here",
    files_here: "{n} files here",
    no_folders: "No folders here",
    opening: "Opening {name}…",
    open_note: "Your games open from the emulator's own file browser.",
    cannot_read_apps: "Cannot read installed apps: {e}",
    wait_install: "Wait for the install to finish",
};

pub static ZH: Words = Words {
    title: "模拟器中心",
    subtitle: "安装模拟器，然后打开你已有的游戏。",
    not_installed: "未安装",
    installed: "已安装 {v}",
    update_to: "可更新到 {v}",
    up_to_date: "已是最新",
    checking: "正在查询商店…",
    offline: "无法连接商店：{e}",
    not_ready: "商店正在检查新版本",
    missing: "商店中暂无",
    waiting: "等待中…",
    downloading: "正在下载 {p}%",
    verifying: "正在校验 {p}%",
    installing: "正在安装…",
    failed: "失败：{e}",
    open: "打开",
    install: "安装",
    update: "更新",
    refresh: "刷新",
    quit: "退出",
    choose: "选择",
    back: "返回",
    games: "游戏",
    tf2: "TF2 卡",
    no_tf2: "未找到 TF2 卡。请选择存放游戏的文件夹。",
    tf2_found: "TF2 卡位于 {p}",
    use_tf2: "使用 TF2 卡",
    change_folder: "选择文件夹",
    picker_title: "选择游戏文件夹",
    use_this: "使用此文件夹",
    places: "位置",
    home: "主目录",
    media: "存储卡和磁盘",
    mounts: "挂载点",
    up: "上一级",
    current: "当前",
    one_file: "1 个文件",
    files: "{n} 个文件",
    one_folder: "1 个文件夹",
    folders: "{n} 个文件夹",
    empty: "空",
    one_file_here: "此处有 1 个文件",
    files_here: "此处有 {n} 个文件",
    no_folders: "这里没有文件夹",
    opening: "正在打开 {name}…",
    open_note: "游戏在模拟器自己的文件浏览器中打开。",
    cannot_read_apps: "无法读取已安装的应用：{e}",
    wait_install: "请等待安装完成",
};

pub fn words(lang: &str) -> &'static Words {
    if lang.starts_with("zh") {
        &ZH
    } else {
        &EN
    }
}

/// Fills `{key}` placeholders.
pub fn fill(template: &str, values: &[(&str, &str)]) -> String {
    values.iter().fold(template.to_string(), |text, (key, value)| text.replace(&format!("{{{key}}}"), value))
}

/// Every character the center can draw, for building its font.
pub fn all_text() -> String {
    let mut text = String::new();
    for w in [&EN, &ZH] {
        for s in [
            w.title,
            w.subtitle,
            w.not_installed,
            w.installed,
            w.update_to,
            w.up_to_date,
            w.checking,
            w.offline,
            w.not_ready,
            w.missing,
            w.waiting,
            w.downloading,
            w.verifying,
            w.installing,
            w.failed,
            w.open,
            w.install,
            w.update,
            w.refresh,
            w.quit,
            w.choose,
            w.back,
            w.games,
            w.tf2,
            w.no_tf2,
            w.tf2_found,
            w.use_tf2,
            w.change_folder,
            w.picker_title,
            w.use_this,
            w.places,
            w.home,
            w.media,
            w.mounts,
            w.up,
            w.current,
            w.one_file,
            w.files,
            w.one_folder,
            w.folders,
            w.empty,
            w.one_file_here,
            w.files_here,
            w.no_folders,
            w.opening,
            w.open_note,
            w.cannot_read_apps,
            w.wait_install,
        ] {
            text.push_str(s);
        }
    }
    for e in crate::catalog::EMULATORS.iter() {
        text.push_str(e.name);
        text.push_str(e.systems);
        text.push_str(e.systems_zh);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill(EN.update_to, &[("v", "1.2")]), "Update to 1.2");
        assert_eq!(words("zh-CN").title, "模拟器中心");
        assert_eq!(words("en").title, "Emulation Center");
    }
}
