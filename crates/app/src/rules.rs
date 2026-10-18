//! 和界面无关的小规则：分支命名、侧栏分支排序、推送目标、提交图要不要重建。

use engine::refs::Refs;

/// 新建分支的命名规则：fix|feat_S|C|SC_小驼峰，如 feat_S_userLogin
pub fn branch_name_ok(name: &str) -> bool {
    let Some((kind, rest)) = name.split_once('_') else { return false };
    let Some((side, word)) = rest.split_once('_') else { return false };
    matches!(kind, "fix" | "feat")
        && matches!(side, "S" | "C" | "SC")
        && word.chars().next().is_some_and(|c| c.is_ascii_lowercase())
        && word.chars().all(|c| c.is_ascii_alphanumeric())
}

/// 侧栏分支的排列：当前分支永远在第一行，其次是固定的分支（按固定的先后），其余保持原来的顺序
pub fn order_branches<T>(items: &mut [T], name: impl Fn(&T) -> &str, head: Option<&str>, pinned: &[String]) {
    let rank = |n: &str| if Some(n) == head { -1 } else { pinned.iter().position(|p| p == n).unwrap_or(pinned.len()) as isize };
    // sort_by_key 是稳定的，同一档里的顺序不变
    items.sort_by_key(|item| rank(name(item)));
}

/// 把上游 `refs/remotes/<远程>/<分支>` 拆开。远程名可以带斜杠，所以用已知的远程列表来拆
pub fn split_upstream(upstream: Option<&str>, remotes: &[String]) -> Option<(String, String)> {
    let upstream = upstream?;
    remotes.iter().find_map(|r| upstream.strip_prefix(&format!("refs/remotes/{r}/")).map(|b| (r.clone(), b.to_owned())))
}

/// 图只依赖提交起点集合；引用改名、已有分支间切换只更新标记和选中行
pub fn graph_key(refs: &Refs) -> Vec<String> {
    let mut ids: Vec<String> = refs.refs.iter().map(|r| r.id.clone()).chain(refs.head_id.clone()).collect();
    ids.sort();
    ids.dedup();
    ids
}

/// refs/heads/x、refs/tags/x、refs/remotes/x 的短名
pub fn short(name: &str) -> &str {
    ["refs/heads/", "refs/tags/", "refs/remotes/"].iter().find_map(|p| name.strip_prefix(p)).unwrap_or(name)
}

/// 版本选择器的分组：当前分支和上游、常用主干、本地分支、远程分支、标签、其他；组内本地优先、按名字排
pub const REVISION_GROUPS: [&str; 6] = ["current", "common", "branches", "remotes", "tags", "other"];

pub struct RevisionOption {
    pub name: String,
    pub label: String,
}

pub fn revision_options(refs: &Refs, query: &str) -> Vec<(&'static str, Vec<RevisionOption>)> {
    let upstream = refs.refs.iter().find(|r| Some(&r.name) == refs.head.as_ref()).and_then(|r| r.upstream.as_deref());
    let needle = query.trim().to_lowercase();
    let mut items: Vec<(&str, bool, RevisionOption)> = refs
        .refs
        .iter()
        .filter(|r| r.name.to_lowercase().contains(&needle))
        .map(|r| {
            let local = r.name.starts_with("refs/heads/");
            let remote = r.name.starts_with("refs/remotes/");
            let label = short(&r.name).to_owned();
            let branch = if remote { label.split_once('/').map_or(label.as_str(), |(_, b)| b) } else { label.as_str() };
            let group = if Some(&r.name) == refs.head.as_ref() || Some(r.name.as_str()) == upstream {
                "current"
            } else if (local || remote) && ["main", "master", "season", "dev", "develop"].contains(&branch) {
                "common"
            } else if local {
                "branches"
            } else if remote {
                "remotes"
            } else if r.name.starts_with("refs/tags/") {
                "tags"
            } else {
                "other"
            };
            (group, local, RevisionOption { name: r.name.clone(), label })
        })
        .collect();
    let head = refs.head.clone();
    items.sort_by(|a, b| {
        let is_head = |x: &RevisionOption| Some(&x.name) == head.as_ref();
        is_head(&b.2).cmp(&is_head(&a.2)).then(b.1.cmp(&a.1)).then(a.2.label.cmp(&b.2.label))
    });
    REVISION_GROUPS
        .iter()
        .map(|g| (*g, items.iter().filter(|(gg, _, _)| gg == g).map(|(_, _, o)| RevisionOption { name: o.name.clone(), label: o.label.clone() }).collect::<Vec<_>>()))
        .filter(|(_, list)| !list.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine::refs::Ref;

    #[test]
    fn branch_names() {
        for ok in ["feat_S_userLogin", "fix_C_a", "feat_SC_x1"] {
            assert!(branch_name_ok(ok), "{ok}");
        }
        for bad in ["feat_S_UserLogin", "feat-S-userLogin", "chore_S_x", "feat_X_x", "feat_S_", "feat_S_user_login"] {
            assert!(!branch_name_ok(bad), "{bad}");
        }
    }

    #[test]
    fn head_then_pinned_then_rest() {
        let mut v = vec!["a", "b", "c", "d"];
        order_branches(&mut v, |s| s, Some("c"), &["d".into(), "b".into()]);
        assert_eq!(v, ["c", "d", "b", "a"]);
    }

    #[test]
    fn upstream_with_slashed_remote() {
        let remotes = vec!["origin".to_owned(), "team/fork".to_owned()];
        assert_eq!(split_upstream(Some("refs/remotes/team/fork/feat/x"), &remotes), Some(("team/fork".into(), "feat/x".into())));
        assert_eq!(split_upstream(Some("refs/remotes/other/x"), &remotes), None);
        assert_eq!(split_upstream(None, &remotes), None);
    }

    #[test]
    fn revision_groups() {
        let r = |name: &str, up: Option<&str>| Ref { name: name.into(), id: "1".into(), upstream: up.map(Into::into) };
        let refs = Refs {
            head: Some("refs/heads/feat".into()),
            head_id: None,
            ahead_behind: None,
            in_progress: None,
            revert_disabled: false,
            refs: vec![r("refs/heads/feat", Some("refs/remotes/origin/feat")), r("refs/remotes/origin/feat", None), r("refs/heads/main", None), r("refs/remotes/origin/main", None), r("refs/heads/zeta", None), r("refs/tags/v1", None)],
        };
        let groups = revision_options(&refs, "");
        let shape: Vec<_> = groups.iter().map(|(g, l)| (*g, l.iter().map(|o| o.label.as_str()).collect::<Vec<_>>())).collect();
        assert_eq!(shape, [("current", vec!["feat", "origin/feat"]), ("common", vec!["main", "origin/main"]), ("branches", vec!["zeta"]), ("tags", vec!["v1"])]);
        assert_eq!(revision_options(&refs, "ZET").len(), 1);
    }

    #[test]
    fn graph_key_ignores_names_and_order() {
        let r = |name: &str, id: &str| Ref { name: name.into(), id: id.into(), upstream: None };
        let a = Refs { head: None, head_id: Some("1".into()), ahead_behind: None, in_progress: None, revert_disabled: false, refs: vec![r("refs/heads/a", "1"), r("refs/heads/b", "2")] };
        let b = Refs { refs: vec![r("refs/heads/renamed", "2"), r("refs/heads/a", "1")], ..a.clone() };
        assert_eq!(graph_key(&a), graph_key(&b));
    }
}
