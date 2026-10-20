mod common;

use engine::{forge, Repo};

#[test]
fn web_address_setting_overrides_the_ssh_remote() {
    let dir = common::init("forge");
    common::git(&dir, 0, &["remote", "add", "origin", "ssh://git@gitlab.example.com:22222/team/proj.git"]);
    let repo = Repo::open(&dir).unwrap();

    // 平台手动指定为 GitLab，不会去探测网络
    forge::set_settings(&repo, "gitlab", " http://gitlab.example.com:8888/ ").unwrap();
    assert_eq!(forge::settings(&repo), ("gitlab".to_owned(), "http://gitlab.example.com:8888".to_owned()));
    let f = forge::detect(&repo).unwrap();
    assert_eq!((f.web.as_str(), f.api.as_str(), f.host.as_str(), f.project.as_str()), ("http://gitlab.example.com:8888", "http://gitlab.example.com:8888/api/v4", "gitlab.example.com", "team/proj"));

    forge::set_settings(&repo, "gitlab", "").unwrap();
    assert_eq!(forge::settings(&repo).1, "");
    assert_eq!(forge::detect(&repo).unwrap().web, "https://gitlab.example.com");
    std::fs::remove_dir_all(&dir).unwrap();
}
